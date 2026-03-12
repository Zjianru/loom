use crate::{LoomHarness, LoomHarnessError, ensure_supported_candidate};
use anyhow::{Result, bail};
use loom_domain::{
    ControlActionKind, InteractionLane, LegacySemanticDecisionEnvelope,
    SemanticDecisionBatchEnvelope, SemanticDecisionEnvelope, SemanticDecisionKind,
    SemanticDecisionPayload,
};
use loom_store::{LoomStoreTx, PersistOutcome};
use std::collections::HashSet;

#[derive(Default)]
struct LegacyDecisionAggregate {
    interaction_decision: Option<SemanticDecisionEnvelope>,
    activation_reason: Option<loom_domain::TaskActivationReason>,
    managed_task_class: Option<loom_domain::ManagedTaskClass>,
    work_horizon: Option<loom_domain::WorkHorizonKind>,
    task_change_classification: Option<loom_domain::TaskChangeClassification>,
    task_change_execution_surface: Option<loom_domain::ChangeExecutionSurface>,
    task_change_boundary_recommendation: Option<loom_domain::BoundaryRecommendation>,
}

fn merge_managed_task_ref(current: &mut Option<String>, candidate: Option<String>) -> Result<()> {
    let Some(candidate) = candidate else {
        return Ok(());
    };
    match current {
        Some(current_ref) if current_ref != &candidate => {
            bail!("semantic bundle managed_task_ref mismatch")
        }
        Some(_) => Ok(()),
        None => {
            *current = Some(candidate);
            Ok(())
        }
    }
}

fn build_legacy_decision(
    batch: &SemanticDecisionBatchEnvelope,
) -> Result<Option<LegacySemanticDecisionEnvelope>> {
    if batch.semantic_decisions.is_empty() {
        return Ok(None);
    }

    let mut seen_refs = HashSet::new();
    let mut seen_kinds = HashSet::new();
    let mut aggregate = LegacyDecisionAggregate::default();
    let mut managed_task_ref = None;

    for decision in &batch.semantic_decisions {
        if !seen_refs.insert(decision.decision_ref.clone()) {
            bail!(LoomHarnessError::DuplicateDecisionRef(
                decision.decision_ref.clone()
            ));
        }
        if !seen_kinds.insert(decision.decision_kind.clone()) {
            bail!(LoomHarnessError::DuplicateSemanticDecisionKind(
                format!("{:?}", decision.decision_kind).to_lowercase()
            ));
        }
        merge_managed_task_ref(&mut managed_task_ref, decision.managed_task_ref.clone())?;
        match (&decision.decision_kind, &decision.decision_payload) {
            (
                SemanticDecisionKind::InteractionLane,
                SemanticDecisionPayload::InteractionLane(_),
            ) => aggregate.interaction_decision = Some(decision.clone()),
            (
                SemanticDecisionKind::TaskActivationReason,
                SemanticDecisionPayload::TaskActivationReason(payload),
            ) => aggregate.activation_reason = Some(payload.task_activation_reason.clone()),
            (
                SemanticDecisionKind::ManagedTaskClass,
                SemanticDecisionPayload::ManagedTaskClass(payload),
            ) => aggregate.managed_task_class = Some(payload.managed_task_class.clone()),
            (SemanticDecisionKind::WorkHorizon, SemanticDecisionPayload::WorkHorizon(payload)) => {
                aggregate.work_horizon = Some(payload.work_horizon.clone())
            }
            (SemanticDecisionKind::TaskChange, SemanticDecisionPayload::TaskChange(payload)) => {
                aggregate.task_change_classification = Some(payload.classification.clone());
                aggregate.task_change_execution_surface = Some(payload.execution_surface.clone());
                aggregate.task_change_boundary_recommendation =
                    Some(payload.boundary_recommendation.clone());
            }
            _ => bail!("semantic decision_kind does not match decision_payload"),
        }
    }

    if let Some(control_action) = batch.control_action.as_ref()
        && !seen_refs.insert(control_action.decision_ref.clone())
    {
        bail!(LoomHarnessError::DuplicateDecisionRef(
            control_action.decision_ref.clone()
        ));
    }

    let Some(interaction_decision) = aggregate.interaction_decision else {
        bail!(LoomHarnessError::MissingInteractionLaneDecision);
    };
    let interaction_decision_ref = interaction_decision.decision_ref;
    let interaction_confidence = interaction_decision.confidence;
    let interaction_issued_at = interaction_decision.issued_at;
    let interaction_host_message_ref = interaction_decision.host_message_ref;
    let SemanticDecisionPayload::InteractionLane(payload) = interaction_decision.decision_payload
    else {
        bail!("interaction_lane decision payload missing");
    };
    merge_managed_task_ref(&mut managed_task_ref, payload.managed_task_ref.clone())?;

    Ok(Some(LegacySemanticDecisionEnvelope {
        decision_id: interaction_decision_ref,
        host_session_id: batch.host_session_id.clone(),
        host_message_ref: interaction_host_message_ref,
        managed_task_ref,
        interaction_lane: payload.interaction_lane,
        managed_task_class: aggregate.managed_task_class,
        work_horizon: aggregate.work_horizon,
        task_activation_reason: aggregate.activation_reason,
        task_change_classification: aggregate.task_change_classification,
        task_change_execution_surface: aggregate.task_change_execution_surface,
        task_change_boundary_recommendation: aggregate.task_change_boundary_recommendation,
        title: payload.title,
        summary: payload.summary,
        expected_outcome: payload.expected_outcome,
        requirement_items: payload.requirement_items,
        workspace_ref: payload.workspace_ref,
        repo_ref: payload.repo_ref,
        allowed_roots: payload.allowed_roots,
        secret_classes: payload.secret_classes,
        confidence: Some(interaction_confidence),
        created_at: interaction_issued_at,
    }))
}

fn known_task_ref_for_clarification(
    batch: &SemanticDecisionBatchEnvelope,
    legacy_decision: Option<&LegacySemanticDecisionEnvelope>,
) -> Option<String> {
    batch
        .control_action
        .as_ref()
        .and_then(|action| action.action.managed_task_ref.clone())
        .or_else(|| legacy_decision.and_then(|decision| decision.managed_task_ref.clone()))
}

fn validate_request_task_change_batch(
    batch: &SemanticDecisionBatchEnvelope,
    legacy_decision: Option<&LegacySemanticDecisionEnvelope>,
) -> Result<()> {
    let Some(control_action) = batch.control_action.as_ref() else {
        return Ok(());
    };
    if control_action.action.kind != ControlActionKind::RequestTaskChange {
        return Ok(());
    }
    if control_action.action.source_decision_ref.is_none() {
        bail!(LoomHarnessError::MissingTaskChangeSourceDecisionRef);
    }
    let Some(legacy_decision) = legacy_decision else {
        bail!(LoomHarnessError::MissingPairedTaskChangeJudgment);
    };
    if legacy_decision.interaction_lane != InteractionLane::ManagedTaskActive {
        bail!(LoomHarnessError::MissingPairedTaskChangeJudgment);
    }
    let source_decision_ref = control_action
        .action
        .source_decision_ref
        .as_ref()
        .expect("source_decision_ref checked");
    let Some(task_change_decision) = batch
        .semantic_decisions
        .iter()
        .find(|decision| &decision.decision_ref == source_decision_ref)
    else {
        bail!(LoomHarnessError::MissingPairedTaskChangeJudgment);
    };
    if task_change_decision.decision_kind != SemanticDecisionKind::TaskChange
        || !matches!(
            task_change_decision.decision_payload,
            SemanticDecisionPayload::TaskChange(_)
        )
    {
        bail!(LoomHarnessError::InvalidTaskChangeSourceDecision);
    }
    if let (Some(action_task_ref), Some(legacy_task_ref)) = (
        control_action.action.managed_task_ref.as_ref(),
        legacy_decision.managed_task_ref.as_ref(),
    ) && action_task_ref != legacy_task_ref
    {
        bail!(LoomHarnessError::TaskChangeManagedTaskMismatch);
    }
    Ok(())
}

fn reject_batch_tx(
    harness: &LoomHarness,
    tx: &mut LoomStoreTx<'_>,
    batch: &SemanticDecisionBatchEnvelope,
    legacy_decision: Option<&LegacySemanticDecisionEnvelope>,
    error: &anyhow::Error,
    request_clarification: bool,
) -> Result<()> {
    tx.save_semantic_decision_batch(batch, "rejected", Some(&error.to_string()))?;
    if request_clarification
        && let Some(managed_task_ref) = known_task_ref_for_clarification(batch, legacy_decision)
    {
        harness.request_task_change_clarification_tx(tx, &managed_task_ref, &error.to_string())?;
    }
    Ok(())
}

impl LoomHarness {
    pub fn ingest_semantic_bundle(&self, batch: SemanticDecisionBatchEnvelope) -> Result<()> {
        let mut tx = self.store.begin_tx()?;
        if !tx.record_ingress_receipt(&batch.meta, "semantic_bundle", &batch)? {
            return Ok(());
        }

        let legacy_decision = match build_legacy_decision(&batch) {
            Ok(legacy_decision) => legacy_decision,
            Err(error) => {
                reject_batch_tx(self, &mut tx, &batch, None, &error, false)?;
                tx.commit()?;
                return Err(error);
            }
        };
        if let Some(legacy_decision) = legacy_decision.as_ref()
            && let Err(error) = ensure_supported_candidate(legacy_decision)
        {
            reject_batch_tx(self, &mut tx, &batch, Some(legacy_decision), &error, false)?;
            tx.commit()?;
            return Err(error);
        }

        if let Err(error) = validate_request_task_change_batch(&batch, legacy_decision.as_ref()) {
            reject_batch_tx(
                self,
                &mut tx,
                &batch,
                legacy_decision.as_ref(),
                &error,
                true,
            )?;
            tx.commit()?;
            return Err(error);
        }

        for decision in &batch.semantic_decisions {
            if tx.semantic_decision_persist_outcome(decision)? == PersistOutcome::Conflict {
                let error =
                    LoomHarnessError::ConflictingDecisionRef(decision.decision_ref.clone()).into();
                reject_batch_tx(
                    self,
                    &mut tx,
                    &batch,
                    legacy_decision.as_ref(),
                    &error,
                    false,
                )?;
                tx.commit()?;
                return Err(error);
            }
        }
        if let Some(control_action) = batch.control_action.as_ref()
            && tx.control_action_envelope_persist_outcome(control_action)?
                == PersistOutcome::Conflict
        {
            let error =
                LoomHarnessError::ConflictingDecisionRef(control_action.decision_ref.clone())
                    .into();
            reject_batch_tx(
                self,
                &mut tx,
                &batch,
                legacy_decision.as_ref(),
                &error,
                false,
            )?;
            tx.commit()?;
            return Err(error);
        }

        tx.save_semantic_decision_batch(&batch, "accepted", None)?;

        let mut has_inserted_semantic_decision = false;
        for decision in &batch.semantic_decisions {
            let outcome = tx.save_semantic_decision(&batch.meta.ingress_id, decision)?;
            has_inserted_semantic_decision |= outcome == PersistOutcome::Inserted;
        }

        if has_inserted_semantic_decision && let Some(legacy_decision) = legacy_decision {
            self.ingest_semantic_decision_tx(&mut tx, legacy_decision)?;
        }

        if let Some(control_action) = batch.control_action {
            let persist_outcome = tx.save_control_action_envelope(&control_action)?;
            if persist_outcome == PersistOutcome::Inserted {
                self.ingest_control_action_tx(&mut tx, control_action.action)?;
            }
        }

        tx.commit()
    }
}

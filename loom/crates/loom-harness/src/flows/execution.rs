use crate::support::{
    SYSTEM_REVIEW_GROUP_REF, build_recorder_command, host_agent_for_role, render_hint_for_result,
};
use crate::LoomHarness;
use anyhow::{Result, anyhow};
use loom_domain::{
    AcceptanceResult, AgentBinding, AgentBindingStatus, AgentRoleKind, ArtifactManifestItem,
    HostExecutionCommand, HostExecutionCommandStatus, HostSubagentLifecycleEnvelope,
    HostSubagentLifecycleEvent, HostSubagentStatus, IsolatedTaskRun, IsolatedTaskRunStatus,
    KernelOutboundPayload, ManagedTask, NextActionItem, ProofEvidenceRef, ProofOfWorkBundle,
    ProofOfWorkExcerpt, ResultContract, ResultOutcome, ResultSummaryPayload, ReviewResult,
    ReviewSummary, ReviewVerdict, SubagentEndedPayload, SubagentSpawnedPayload, WorkflowStage,
    new_id, now_timestamp,
};
use std::path::{Component, Path, PathBuf};

impl LoomHarness {
    pub fn ingest_subagent_lifecycle(&self, envelope: HostSubagentLifecycleEnvelope) -> Result<()> {
        if !self
            .store
            .record_ingress_receipt(&envelope.meta, "subagent_lifecycle", &envelope)?
        {
            return Ok(());
        }
        self.store.save_host_subagent_lifecycle_event(&envelope)?;
        let mut command = self
            .store
            .load_host_execution_command(&envelope.command_id)?
            .ok_or_else(|| anyhow!("host execution command not found: {}", envelope.command_id))?;
        let mut task = self
            .store
            .load_managed_task(&envelope.managed_task_ref)?
            .ok_or_else(|| anyhow!("managed task not found: {}", envelope.managed_task_ref))?;
        let mut run = self
            .store
            .load_task_run(&envelope.run_ref)?
            .ok_or_else(|| anyhow!("task run not found: {}", envelope.run_ref))?;

        match &envelope.event {
            HostSubagentLifecycleEvent::Spawned(payload) => {
                self.apply_spawned_event(&mut command, &task, payload)?;
            }
            HostSubagentLifecycleEvent::Ended(payload) => {
                self.apply_ended_event(&mut command, &task, payload)?;
                match command.role_kind {
                    AgentRoleKind::Worker => {
                        self.finish_worker_execution(&mut task, &mut run, &command, payload)?
                    }
                    AgentRoleKind::Recorder => {
                        self.finish_recorder_execution(&mut task, &mut run, &command, payload)?
                    }
                    AgentRoleKind::Net => {}
                }
            }
        }
        Ok(())
    }

    pub(crate) fn finalize_task_result(
        &self,
        task: &mut ManagedTask,
        run: &mut IsolatedTaskRun,
        binding: &AgentBinding,
        summary_text: String,
        artifact_refs: Vec<String>,
        review: ReviewResult,
        outcome: ResultOutcome,
        acceptance_verdict: AcceptanceResult,
    ) -> Result<()> {
        let scope_version = task.current_scope_version.unwrap_or(1);
        let artifact_manifest = artifact_refs
            .iter()
            .cloned()
            .map(|reference| ArtifactManifestItem {
                label: "artifact".into(),
                reference,
            })
            .collect::<Vec<_>>();
        let proof = ProofOfWorkBundle {
            proof_of_work_id: new_id("proof"),
            managed_task_ref: task.managed_task_ref.clone(),
            run_ref: run.run_ref.clone(),
            acceptance_verdict: acceptance_verdict.clone(),
            acceptance_basis: acceptance_basis_for_outcome(&outcome, &review),
            accepted_by_ref: SYSTEM_REVIEW_GROUP_REF.into(),
            accepted_at: now_timestamp(),
            accepted_scope_version: scope_version,
            review_summary: review.summary.clone(),
            artifact_manifest: artifact_manifest.clone(),
            evidence_refs: vec![
                ProofEvidenceRef {
                    label: "run_ref".into(),
                    reference: run.run_ref.clone(),
                },
                ProofEvidenceRef {
                    label: "review_result".into(),
                    reference: review.review_result_id.clone(),
                },
            ],
            run_summary: summary_text.clone(),
        };
        let next_actions = next_actions_for_outcome(&outcome, &review);
        let contract = ResultContract {
            result_contract_id: new_id("result"),
            managed_task_ref: task.managed_task_ref.clone(),
            pack_ref: Some(crate::support::DEFAULT_PACK_REF.into()),
            outcome: outcome.clone(),
            acceptance_verdict: acceptance_verdict.clone(),
            final_scope_version: scope_version,
            scope_revision_summary: Some(format!("scope_version={scope_version}")),
            summary: summary_text.clone(),
            key_outcomes: build_key_outcomes(&summary_text, task),
            proof_of_work: proof.clone(),
            next_actions: next_actions.clone(),
            created_at: now_timestamp(),
        };
        self.store.save_proof_of_work_bundle(&proof)?;
        self.store.save_result_contract(&contract)?;
        self.store.save_review_result(&review)?;

        task.workflow_stage = WorkflowStage::Result;
        task.review_result = Some(review.clone());
        task.proof_of_work_bundle = Some(proof.clone());
        task.result_contract = Some(contract.clone());
        task.agent_binding = Some(AgentBinding {
            status: AgentBindingStatus::Closed,
            ..binding.clone()
        });
        task.updated_at = now_timestamp();
        run.status = IsolatedTaskRunStatus::Result;
        run.updated_at = now_timestamp();
        self.store.save_task_run(run)?;
        if let Some(binding) = task.agent_binding.as_ref() {
            self.store.save_agent_binding(binding)?;
        }
        self.store.save_managed_task(task)?;

        self.store.enqueue_outbound(
            task.host_session_id.clone(),
            KernelOutboundPayload::ResultSummary(ResultSummaryPayload {
                managed_task_ref: task.managed_task_ref.clone(),
                outcome,
                acceptance_verdict,
                summary: contract.summary.clone(),
                final_scope_version: contract.final_scope_version,
                scope_revision_headline: contract.scope_revision_summary.clone(),
                proof_of_work_excerpt: ProofOfWorkExcerpt {
                    run_summary: proof.run_summary.clone(),
                    evidence_refs: proof.evidence_refs.clone(),
                    review_summary: Some(proof.review_summary.clone()),
                    artifact_manifest_excerpt: proof.artifact_manifest.clone(),
                    acceptance_basis_excerpt: proof.acceptance_basis.clone(),
                },
                next_actions_excerpt: contract.next_actions.clone(),
                render_hint: render_hint_for_result(),
            }),
        )?;
        self.log_event(
            &task.managed_task_ref,
            "result_contract.created",
            serde_json::json!({
                "managed_task_ref": task.managed_task_ref,
                "result_contract_id": contract.result_contract_id,
                "outcome": contract.outcome,
                "acceptance_verdict": contract.acceptance_verdict,
            }),
        )?;
        Ok(())
    }

    fn apply_spawned_event(
        &self,
        command: &mut HostExecutionCommand,
        task: &ManagedTask,
        payload: &SubagentSpawnedPayload,
    ) -> Result<()> {
        command.status = HostExecutionCommandStatus::Running;
        command.host_child_execution_ref = Some(payload.host_child_execution_ref.clone());
        command.host_child_run_ref = payload.host_child_run_ref.clone();
        self.store.save_host_execution_command(command)?;
        self.log_event(
            &task.managed_task_ref,
            "host_execution_spawned",
            serde_json::json!({
                "managed_task_ref": task.managed_task_ref,
                "command_id": command.command_id,
                "binding_id": command.binding_id,
                "role_kind": command.role_kind,
                "host_session_id": command.host_session_id,
                "host_child_execution_ref": payload.host_child_execution_ref,
                "host_child_run_ref": payload.host_child_run_ref,
                "host_agent_id": payload.host_agent_id,
                "artifact_refs": [],
                "terminal_status": serde_json::Value::Null,
            }),
        )?;
        Ok(())
    }

    fn apply_ended_event(
        &self,
        command: &mut HostExecutionCommand,
        task: &ManagedTask,
        payload: &SubagentEndedPayload,
    ) -> Result<()> {
        command.host_child_execution_ref = Some(payload.host_child_execution_ref.clone());
        command.host_child_run_ref = payload.host_child_run_ref.clone();
        command.output_summary = Some(payload.output_summary.clone());
        command.artifact_refs = payload.artifact_refs.clone();
        command.completed_at = Some(payload.observed_at.clone());
        command.status = match payload.status {
            HostSubagentStatus::Completed => HostExecutionCommandStatus::Completed,
            HostSubagentStatus::Failed
            | HostSubagentStatus::TimedOut
            | HostSubagentStatus::Cancelled => HostExecutionCommandStatus::Failed,
        };
        self.store.save_host_execution_command(command)?;
        self.log_event(
            &task.managed_task_ref,
            match payload.status {
                HostSubagentStatus::Completed => "host_execution_completed",
                HostSubagentStatus::Failed
                | HostSubagentStatus::TimedOut
                | HostSubagentStatus::Cancelled => "host_execution_failed",
            },
            serde_json::json!({
                "managed_task_ref": task.managed_task_ref,
                "command_id": command.command_id,
                "binding_id": command.binding_id,
                "role_kind": command.role_kind,
                "host_session_id": command.host_session_id,
                "host_child_execution_ref": payload.host_child_execution_ref,
                "host_child_run_ref": payload.host_child_run_ref,
                "host_agent_id": payload.host_agent_id,
                "status": payload.status,
                "artifact_refs": payload.artifact_refs,
                "terminal_status": payload.status,
            }),
        )?;
        Ok(())
    }

    fn finish_worker_execution(
        &self,
        task: &mut ManagedTask,
        run: &mut IsolatedTaskRun,
        command: &HostExecutionCommand,
        payload: &SubagentEndedPayload,
    ) -> Result<()> {
        task.workflow_stage = WorkflowStage::Review;
        task.updated_at = now_timestamp();
        run.status = IsolatedTaskRunStatus::Review;
        run.updated_at = now_timestamp();

        let binding = self
            .store
            .latest_agent_binding(&task.managed_task_ref)?
            .ok_or_else(|| anyhow!("agent binding missing for task {}", task.managed_task_ref))?;
        let authorization_id = task
            .current_execution_authorization_ref
            .clone()
            .ok_or_else(|| anyhow!("execution authorization missing for task {}", task.managed_task_ref))?;
        let authorization = self
            .store
            .load_execution_authorization(&authorization_id)?
            .ok_or_else(|| anyhow!("authorization payload missing for task {}", task.managed_task_ref))?;
        let review = build_review_result(task, run, command, payload, &authorization)?;
        self.store.save_review_result(&review)?;
        self.store.save_task_run(run)?;
        task.review_result = Some(review.clone());
        self.store.save_managed_task(task)?;

        if review.review_verdict == ReviewVerdict::Approved {
            let spec = task
                .spec_bundle
                .clone()
                .ok_or_else(|| anyhow!("spec bundle missing for task {}", task.managed_task_ref))?;
            let recorder_command = build_recorder_command(
                task,
                &run.run_ref,
                &binding,
                &spec,
                command.output_summary.as_deref().unwrap_or_default(),
                &review,
            );
            self.store.enqueue_host_execution_command(&recorder_command)?;
            self.log_event(
                &task.managed_task_ref,
                "host_execution_command_queued",
                serde_json::json!({
                    "managed_task_ref": task.managed_task_ref,
                    "command_id": recorder_command.command_id,
                    "binding_id": recorder_command.binding_id,
                    "role_kind": recorder_command.role_kind,
                    "host_session_id": recorder_command.host_session_id,
                    "host_agent_id": host_agent_for_role(&binding, AgentRoleKind::Recorder),
                    "host_child_execution_ref": serde_json::Value::Null,
                    "host_child_run_ref": serde_json::Value::Null,
                    "artifact_refs": [],
                    "terminal_status": serde_json::Value::Null,
                }),
            )?;
            return Ok(());
        }

        self.finalize_task_result(
            task,
            run,
            &binding,
            review.summary.summary.clone(),
            command.artifact_refs.clone(),
            review.clone(),
            review_outcome(&review.review_verdict),
            review_acceptance(&review.review_verdict),
        )
    }

    fn finish_recorder_execution(
        &self,
        task: &mut ManagedTask,
        run: &mut IsolatedTaskRun,
        command: &HostExecutionCommand,
        payload: &SubagentEndedPayload,
    ) -> Result<()> {
        let binding = self
            .store
            .latest_agent_binding(&task.managed_task_ref)?
            .ok_or_else(|| anyhow!("agent binding missing for task {}", task.managed_task_ref))?;
        let review = task
            .review_result
            .clone()
            .or_else(|| self.store.latest_review_result(&task.managed_task_ref).ok().flatten())
            .ok_or_else(|| anyhow!("review result missing for task {}", task.managed_task_ref))?;
        let outcome = match payload.status {
            HostSubagentStatus::Completed => ResultOutcome::Completed,
            HostSubagentStatus::Failed
            | HostSubagentStatus::TimedOut
            | HostSubagentStatus::Cancelled => ResultOutcome::Blocked,
        };
        let acceptance = match outcome {
            ResultOutcome::Completed => AcceptanceResult::Accepted,
            ResultOutcome::ReworkRequired => AcceptanceResult::ReworkRequired,
            ResultOutcome::Blocked => AcceptanceResult::EscalatedToUser,
        };
        self.finalize_task_result(
            task,
            run,
            &binding,
            command
                .output_summary
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| review.summary.summary.clone()),
            command.artifact_refs.clone(),
            review,
            outcome,
            acceptance,
        )
    }
}

fn build_review_result(
    task: &ManagedTask,
    run: &IsolatedTaskRun,
    command: &HostExecutionCommand,
    payload: &SubagentEndedPayload,
    authorization: &loom_domain::ExecutionAuthorization,
) -> Result<ReviewResult> {
    let spec = task
        .spec_bundle
        .as_ref()
        .ok_or_else(|| anyhow!("spec bundle missing for task {}", task.managed_task_ref))?;
    let output_summary = payload.output_summary.trim();
    let mut findings: Vec<String> = Vec::new();
    if output_summary.is_empty() {
        findings.push("worker returned empty output".into());
    }
    if !output_summary.to_ascii_lowercase().contains("verification") {
        findings.push("worker output is missing the required Verification section".into());
    }
    if !artifact_refs_within_authorized_roots(&payload.artifact_refs, authorization) {
        findings.push("worker artifacts escaped authorized roots".into());
    }
    if task.current_scope_ref.as_deref() != Some(spec.task_scope_ref.as_str()) {
        findings.push("task scope changed while worker result was in flight".into());
    }
    let review_verdict = if matches!(
        payload.status,
        HostSubagentStatus::Failed | HostSubagentStatus::TimedOut | HostSubagentStatus::Cancelled
    ) {
        ReviewVerdict::Blocked
    } else if findings
        .iter()
        .any(|finding| finding.contains("authorized roots") || finding.contains("scope changed"))
    {
        ReviewVerdict::Blocked
    } else if findings.is_empty() {
        ReviewVerdict::Approved
    } else {
        ReviewVerdict::ReworkRequired
    };
    let summary_text = match review_verdict {
        ReviewVerdict::Approved => "Worker output passed the first-round review gate.".into(),
        ReviewVerdict::ReworkRequired => "Worker output needs another execute pass before delivery.".into(),
        ReviewVerdict::Blocked => "Worker output could not be accepted because execution drifted outside the approved gate.".into(),
    };
    Ok(ReviewResult {
        review_result_id: new_id("review"),
        managed_task_ref: task.managed_task_ref.clone(),
        run_ref: run.run_ref.clone(),
        reviewer_group_ref: SYSTEM_REVIEW_GROUP_REF.into(),
        review_verdict: review_verdict.clone(),
        findings: findings.clone(),
        review_artifacts: command
            .artifact_refs
            .iter()
            .cloned()
            .map(|reference| ArtifactManifestItem {
                label: "worker_artifact".into(),
                reference,
            })
            .collect(),
        summary: ReviewSummary {
            review_verdict,
            summary: summary_text,
            key_findings: findings,
            follow_up_required: !matches!(payload.status, HostSubagentStatus::Completed)
                || !output_summary.to_ascii_lowercase().contains("verification"),
        },
        reviewed_at: now_timestamp(),
    })
}

fn artifact_refs_within_authorized_roots(
    artifact_refs: &[String],
    authorization: &loom_domain::ExecutionAuthorization,
) -> bool {
    let allowed_roots = authorization
        .granted_areas
        .iter()
        .find(|area| area.decision_area == loom_domain::DecisionArea::TaskExecution)
        .map(|area| {
            area.readable_roots
                .iter()
                .chain(area.writable_roots.iter())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    artifact_refs.iter().all(|reference| {
        if reference.trim().is_empty() {
            return false;
        }
        let path = Path::new(reference);
        if path.is_absolute() {
            return allowed_roots.iter().any(|root| path.starts_with(root));
        }
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return false;
        }
        allowed_roots.iter().any(|root| root.join(path).starts_with(root))
    })
}

fn review_outcome(review_verdict: &ReviewVerdict) -> ResultOutcome {
    match review_verdict {
        ReviewVerdict::Approved => ResultOutcome::Completed,
        ReviewVerdict::ReworkRequired => ResultOutcome::ReworkRequired,
        ReviewVerdict::Blocked => ResultOutcome::Blocked,
    }
}

fn review_acceptance(review_verdict: &ReviewVerdict) -> AcceptanceResult {
    match review_verdict {
        ReviewVerdict::Approved => AcceptanceResult::Accepted,
        ReviewVerdict::ReworkRequired => AcceptanceResult::ReworkRequired,
        ReviewVerdict::Blocked => AcceptanceResult::EscalatedToUser,
    }
}

fn acceptance_basis_for_outcome(outcome: &ResultOutcome, review: &ReviewResult) -> Vec<String> {
    match outcome {
        ResultOutcome::Completed => vec![
            "Review approved the worker execution.".into(),
            "Recorder produced a deliverable summary.".into(),
            review.summary.summary.clone(),
        ],
        ResultOutcome::ReworkRequired => vec![
            "Review found gaps that require another execute pass.".into(),
            review.summary.summary.clone(),
        ],
        ResultOutcome::Blocked => vec![
            "Execution could not remain inside the first-round closed loop.".into(),
            review.summary.summary.clone(),
        ],
    }
}

fn next_actions_for_outcome(outcome: &ResultOutcome, review: &ReviewResult) -> Vec<NextActionItem> {
    match outcome {
        ResultOutcome::Completed => vec![NextActionItem {
            title: "Verify delivered summary".into(),
            details: "Confirm the WebUI result summary and artifact list match the task intent.".into(),
        }],
        ResultOutcome::ReworkRequired => vec![NextActionItem {
            title: "Schedule rework".into(),
            details: review
                .findings
                .first()
                .cloned()
                .unwrap_or_else(|| "Review requested another execute pass.".into()),
        }],
        ResultOutcome::Blocked => vec![NextActionItem {
            title: "Inspect blocked execution".into(),
            details: review
                .findings
                .first()
                .cloned()
                .unwrap_or_else(|| "Bridge or scope state prevented delivery.".into()),
        }],
    }
}

fn build_key_outcomes(summary_text: &str, task: &ManagedTask) -> Vec<String> {
    let primary_line = summary_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(summary_text);
    vec![primary_line.trim().to_string(), task.expected_outcome.clone()]
}

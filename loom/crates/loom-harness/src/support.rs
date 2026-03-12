use crate::LoomHarness;
use anyhow::{Result, anyhow};
use loom_domain::{
    AgentBinding, AgentBindingMember, AgentBindingStatus, AgentExecutionMode, AgentRoleKind,
    ControlActionKind, HostExecutionCommand, HostExecutionCommandStatus, ManagedTask,
    PhaseEntryOrigin, PhasePlan, PhasePlanEntry, PhasePlanMetadata, PhasePlanMutationPolicy,
    PhasePlanSource, RenderEmphasis, RenderHint, RenderTone, RequirementItem, RequirementItemDraft,
    RequirementOrigin, ReviewResult, SpecBundle, StageVisibility, StatusNoticeKind,
    StatusNoticePayload, TaskEvent, TaskScopeSnapshot, WorkflowStage, new_id, now_timestamp,
};
use loom_store::LoomStoreTx;

pub(crate) const DEFAULT_PACK_REF: &str = "coding_pack";
pub(crate) const SYSTEM_REVIEW_GROUP_REF: &str = "system.review.v0";

impl LoomHarness {
    pub(crate) fn log_event(
        &self,
        managed_task_ref: &str,
        event_name: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        self.store.append_task_event(TaskEvent {
            event_id: new_id("event"),
            managed_task_ref: managed_task_ref.to_string(),
            event_name: event_name.to_string(),
            payload,
            recorded_at: now_timestamp(),
        })
    }

    pub(crate) fn log_event_tx(
        &self,
        tx: &mut LoomStoreTx<'_>,
        managed_task_ref: &str,
        event_name: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        tx.append_task_event(TaskEvent {
            event_id: new_id("event"),
            managed_task_ref: managed_task_ref.to_string(),
            event_name: event_name.to_string(),
            payload,
            recorded_at: now_timestamp(),
        })
    }

    pub(crate) fn request_task_change_clarification_tx(
        &self,
        tx: &mut LoomStoreTx<'_>,
        managed_task_ref: &str,
        reason: &str,
    ) -> Result<()> {
        let task = tx
            .load_managed_task(&managed_task_ref.to_string())?
            .ok_or_else(|| anyhow!("managed task missing for clarification: {managed_task_ref}"))?;
        self.log_event_tx(
            tx,
            managed_task_ref,
            "task_change_clarification_requested",
            serde_json::json!({
                "managed_task_ref": managed_task_ref,
                "reason": reason,
                "questions": [
                    "What should change in the current task?",
                    "Why should it change, and what outcome do you expect?",
                    "Should current work continue, pause, or rework?"
                ],
            }),
        )?;
        let Some(phase_plan) = task.phase_plan.clone() else {
            return Ok(());
        };
        let Some(stage_package_id) = stage_package_for_workflow_stage(&task.workflow_stage) else {
            return Ok(());
        };
        tx.enqueue_outbound(
            task.host_session_id.clone(),
            loom_domain::KernelOutboundPayload::StatusNotice(build_status_notice(
                &task.managed_task_ref,
                &phase_plan,
                stage_package_id,
                StatusNoticeKind::Blocked,
                "Task change needs clarification",
                "Formal task change request is blocked until the requested modification is clarified.",
                Some(
                    "Please clarify what should change, why it should change, and whether current work should continue, pause, or rework.",
                ),
            )?),
        )?;
        Ok(())
    }
}

pub(crate) fn requires_decision_token(kind: &ControlActionKind) -> bool {
    matches!(
        kind,
        ControlActionKind::ApproveStart
            | ControlActionKind::ModifyCandidate
            | ControlActionKind::CancelCandidate
            | ControlActionKind::KeepCurrentTask
            | ControlActionKind::ReplaceActive
            | ControlActionKind::ApproveRequest
            | ControlActionKind::RejectRequest
    )
}

pub(crate) fn to_requirement_items(
    drafts: &[RequirementItemDraft],
    origin: RequirementOrigin,
) -> Vec<RequirementItem> {
    drafts
        .iter()
        .map(|draft| RequirementItem::accepted(draft.text.clone(), origin.clone()))
        .collect()
}

pub(crate) fn build_spec_bundle(task: &ManagedTask, scope: &TaskScopeSnapshot) -> SpecBundle {
    let scope_doc = [
        format!("Task ref: {}", task.managed_task_ref),
        format!("Scope version: {}", scope.scope_version),
        format!(
            "Workspace: {}",
            scope
                .workspace_ref
                .clone()
                .unwrap_or_else(|| "(none)".into())
        ),
        format!(
            "Repo: {}",
            scope.repo_ref.clone().unwrap_or_else(|| "(none)".into())
        ),
        format!("Allowed roots: {}", scope.allowed_roots.join(", ")),
        format!("Secret classes: {}", scope.secret_classes.join(", ")),
        "Requirements:".into(),
    ]
    .into_iter()
    .chain(
        scope
            .requirement_items
            .iter()
            .map(|item| format!("- {}", item.text)),
    )
    .collect::<Vec<_>>()
    .join("\n");
    let plan_doc = [
        "Phase sequence: clarify -> execute -> review -> deliver".into(),
        format!("Title: {}", task.title),
        format!("Summary: {}", task.summary),
        format!("Expected outcome: {}", task.expected_outcome),
        "Execution contract: produce a concise implementation summary, changed artifacts, and a verification section.".into(),
    ]
    .join("\n");
    let verification_doc = [
        String::from("Return a section starting with `Verification:`."),
        String::from("Include checks actually run, or `Verification: not run` with reason."),
        String::from("List changed files using absolute or workspace-relative paths."),
    ]
    .join("\n");
    SpecBundle {
        spec_bundle_id: new_id("spec"),
        managed_task_ref: task.managed_task_ref.clone(),
        task_scope_ref: scope.scope_id.clone(),
        summary: scope.scope_summary.clone(),
        scope_doc,
        plan_doc,
        verification_doc,
        created_at: now_timestamp(),
    }
}

pub(crate) fn build_default_phase_plan(managed_task_ref: &str) -> PhasePlan {
    let clarify_entry_id = new_id("phase-entry");
    let execute_entry_id = new_id("phase-entry");
    let review_entry_id = new_id("phase-entry");
    let deliver_entry_id = new_id("phase-entry");
    PhasePlan {
        phase_plan_id: new_id("phase-plan"),
        managed_task_ref: managed_task_ref.into(),
        plan_source: PhasePlanSource::SystemDefault,
        plan_entries: vec![
            PhasePlanEntry {
                entry_id: clarify_entry_id,
                stage_package_id: "clarify".into(),
                sequence_no: 1,
                visibility: StageVisibility::UserVisible,
                origin: PhaseEntryOrigin::SystemInserted,
                required: true,
                skip_allowed: false,
                rework_target: None,
            },
            PhasePlanEntry {
                entry_id: execute_entry_id.clone(),
                stage_package_id: "execute".into(),
                sequence_no: 2,
                visibility: StageVisibility::UserVisible,
                origin: PhaseEntryOrigin::PackDefault,
                required: true,
                skip_allowed: false,
                rework_target: None,
            },
            PhasePlanEntry {
                entry_id: review_entry_id,
                stage_package_id: "review".into(),
                sequence_no: 3,
                visibility: StageVisibility::Internal,
                origin: PhaseEntryOrigin::SystemInserted,
                required: true,
                skip_allowed: false,
                rework_target: Some(execute_entry_id),
            },
            PhasePlanEntry {
                entry_id: deliver_entry_id,
                stage_package_id: "deliver".into(),
                sequence_no: 4,
                visibility: StageVisibility::UserVisible,
                origin: PhaseEntryOrigin::SystemInserted,
                required: true,
                skip_allowed: false,
                rework_target: None,
            },
        ],
        mutation_policy: PhasePlanMutationPolicy {
            user_adjustment_allowed: false,
            system_insert_allowed: true,
        },
        metadata: PhasePlanMetadata {
            pack_ref: Some(DEFAULT_PACK_REF.into()),
            default_stage_sequence: vec![
                "clarify".into(),
                "execute".into(),
                "review".into(),
                "deliver".into(),
            ],
        },
        created_at: now_timestamp(),
    }
}

pub(crate) fn stage_package_for_workflow_stage(
    workflow_stage: &WorkflowStage,
) -> Option<&'static str> {
    match workflow_stage {
        WorkflowStage::Candidate => Some("clarify"),
        WorkflowStage::Execute => Some("execute"),
        WorkflowStage::Review => Some("review"),
        WorkflowStage::Result => Some("deliver"),
        WorkflowStage::Closed => None,
    }
}

pub(crate) fn build_agent_binding(
    managed_task: &ManagedTask,
    run_ref: &str,
    capability_snapshot_ref: &str,
) -> AgentBinding {
    AgentBinding {
        binding_id: new_id("binding"),
        managed_task_ref: managed_task.managed_task_ref.clone(),
        run_ref: run_ref.into(),
        stage_run_ref: None,
        pack_ref: Some(DEFAULT_PACK_REF.into()),
        capability_snapshot_ref: capability_snapshot_ref.into(),
        members: vec![
            AgentBindingMember {
                role_kind: AgentRoleKind::Net,
                profile_ref: "main".into(),
                host_mapping_ref: Some("main".into()),
                responsibilities: vec!["Host bridge coordination".into()],
                execution_mode: AgentExecutionMode::Inline,
                required: true,
            },
            AgentBindingMember {
                role_kind: AgentRoleKind::Worker,
                profile_ref: "coder".into(),
                host_mapping_ref: Some("coder".into()),
                responsibilities: vec!["Implement the approved task scope".into()],
                execution_mode: AgentExecutionMode::BackgroundWorker,
                required: true,
            },
            AgentBindingMember {
                role_kind: AgentRoleKind::Recorder,
                profile_ref: "product_analyst".into(),
                host_mapping_ref: Some("product_analyst".into()),
                responsibilities: vec!["Summarize outcome and proof of work".into()],
                execution_mode: AgentExecutionMode::RecorderOnly,
                required: true,
            },
        ],
        status: AgentBindingStatus::Active,
        issued_reason: "approve_start fixed first-round binding".into(),
        issued_at: now_timestamp(),
        supersedes: None,
    }
}

pub(crate) fn host_agent_for_role(binding: &AgentBinding, role_kind: AgentRoleKind) -> String {
    binding
        .members
        .iter()
        .find(|member| member.role_kind == role_kind)
        .and_then(|member| member.host_mapping_ref.clone())
        .unwrap_or_else(|| match role_kind {
            AgentRoleKind::Net => "main".into(),
            AgentRoleKind::Worker => "coder".into(),
            AgentRoleKind::Recorder => "product_analyst".into(),
        })
}

pub(crate) fn build_worker_command(
    task: &ManagedTask,
    run_ref: &str,
    binding: &AgentBinding,
    spec: &SpecBundle,
) -> HostExecutionCommand {
    let prompt = [
        "Role: worker",
        "Execute the approved managed task and do not discuss governance internals.",
        "",
        "Scope doc:",
        &spec.scope_doc,
        "",
        "Plan doc:",
        &spec.plan_doc,
        "",
        "Verification doc:",
        &spec.verification_doc,
        "",
        "Output format:",
        "1. Summary: ...",
        "2. Changed files: ...",
        "3. Verification: ...",
        "",
        &format!("Expected outcome: {}", task.expected_outcome),
    ]
    .join("\n");
    HostExecutionCommand {
        command_id: new_id("exec"),
        managed_task_ref: task.managed_task_ref.clone(),
        run_ref: run_ref.into(),
        binding_id: binding.binding_id.clone(),
        role_kind: AgentRoleKind::Worker,
        host_session_id: task.host_session_id.clone(),
        host_agent_id: host_agent_for_role(binding, AgentRoleKind::Worker),
        prompt,
        label: format!("loom-worker-{}", task.managed_task_ref),
        status: HostExecutionCommandStatus::Pending,
        host_child_execution_ref: None,
        host_child_run_ref: None,
        output_summary: None,
        artifact_refs: Vec::new(),
        issued_at: now_timestamp(),
        acked_at: None,
        completed_at: None,
    }
}

pub(crate) fn build_recorder_command(
    task: &ManagedTask,
    run_ref: &str,
    binding: &AgentBinding,
    spec: &SpecBundle,
    worker_output_summary: &str,
    review: &ReviewResult,
) -> HostExecutionCommand {
    let prompt = [
        "Role: recorder",
        "Summarize the completed task for the user with concise evidence.",
        "",
        "Spec summary:",
        &spec.summary,
        "",
        "Worker output summary:",
        worker_output_summary,
        "",
        "Review summary:",
        &review.summary.summary,
        "",
        "Output format:",
        "1. Summary: ...",
        "2. Key outcomes: ...",
        "3. Proof excerpt: ...",
        "4. Next actions: ...",
        "",
        &format!("Task title: {}", task.title),
    ]
    .join("\n");
    HostExecutionCommand {
        command_id: new_id("exec"),
        managed_task_ref: task.managed_task_ref.clone(),
        run_ref: run_ref.into(),
        binding_id: binding.binding_id.clone(),
        role_kind: AgentRoleKind::Recorder,
        host_session_id: task.host_session_id.clone(),
        host_agent_id: host_agent_for_role(binding, AgentRoleKind::Recorder),
        prompt,
        label: format!("loom-recorder-{}", task.managed_task_ref),
        status: HostExecutionCommandStatus::Pending,
        host_child_execution_ref: None,
        host_child_run_ref: None,
        output_summary: None,
        artifact_refs: Vec::new(),
        issued_at: now_timestamp(),
        acked_at: None,
        completed_at: None,
    }
}

pub(crate) fn render_hint_for_result() -> RenderHint {
    RenderHint::default()
}

pub(crate) fn build_status_notice(
    managed_task_ref: &str,
    phase_plan: &PhasePlan,
    stage_package_id: &str,
    notice_kind: StatusNoticeKind,
    headline: &str,
    summary: &str,
    detail: Option<&str>,
) -> Result<StatusNoticePayload> {
    let stage_entry = phase_plan
        .plan_entries
        .iter()
        .find(|entry| entry.stage_package_id == stage_package_id)
        .ok_or_else(|| anyhow!("phase entry missing for stage package: {stage_package_id}"))?;
    if notice_kind == StatusNoticeKind::StageEntered
        && stage_entry.visibility != StageVisibility::UserVisible
    {
        return Err(anyhow!(
            "stage_entered notice requires user-visible phase entry: {stage_package_id}"
        ));
    }
    Ok(StatusNoticePayload {
        managed_task_ref: managed_task_ref.into(),
        notice_kind: notice_kind.clone(),
        stage_ref: stage_entry.entry_id.clone(),
        headline: headline.into(),
        summary: summary.into(),
        detail: detail.map(str::to_string),
        render_hint: render_hint_for_status_notice(notice_kind),
    })
}

fn render_hint_for_status_notice(notice_kind: StatusNoticeKind) -> RenderHint {
    match notice_kind {
        StatusNoticeKind::StageEntered => RenderHint {
            tone: RenderTone::Neutral,
            emphasis: RenderEmphasis::Minimal,
            ..RenderHint::default()
        },
        StatusNoticeKind::Blocked => RenderHint {
            tone: RenderTone::Blocking,
            emphasis: RenderEmphasis::Strong,
            ..RenderHint::default()
        },
    }
}

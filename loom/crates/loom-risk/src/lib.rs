use loom_domain::{
    AuthorizationBudgetBand, DecisionArea, EvidenceRef, HostCapabilitySnapshot, ManagedTask,
    ProposedAction, RiskAssessment, RiskBand, RiskConsequence, RiskDimensions, RiskSubjectKind,
    TaskScopeSnapshot, new_id, now_timestamp,
};

pub fn assess_task_baseline(
    managed_task: &ManagedTask,
    scope: &TaskScopeSnapshot,
    capability: &HostCapabilitySnapshot,
    trigger_reason: impl Into<String>,
    supersedes: Option<String>,
) -> RiskAssessment {
    let write_impact = if scope.allowed_roots.iter().any(|root| root == "/") {
        RiskBand::Critical
    } else if capability.writable_roots.is_empty() {
        RiskBand::Low
    } else {
        RiskBand::Medium
    };
    let secret_exposure = if scope
        .secret_classes
        .iter()
        .any(|class| class.contains("prod") || class.contains("sensitive"))
    {
        RiskBand::High
    } else if scope.secret_classes.is_empty() {
        RiskBand::Low
    } else {
        RiskBand::Medium
    };
    let dimensions = RiskDimensions {
        write_impact,
        external_side_effect: RiskBand::Low,
        secret_exposure,
        irreversibility: RiskBand::Low,
        budget_burn: match capability.max_budget_band {
            AuthorizationBudgetBand::Conservative => RiskBand::Low,
            AuthorizationBudgetBand::Standard => RiskBand::Medium,
            AuthorizationBudgetBand::Elevated => RiskBand::High,
        },
        uncertainty: RiskBand::Medium,
    };
    let overall_risk_band = dimensions.max_band();
    let mut derived_consequences = vec![];
    if overall_risk_band >= RiskBand::High {
        derived_consequences.push(RiskConsequence::MountAdditionalGovernance);
    }
    if overall_risk_band == RiskBand::Critical {
        derived_consequences.push(RiskConsequence::RequireApprovalWindow);
        derived_consequences.push(RiskConsequence::StopSilentExecution);
    }
    RiskAssessment {
        assessment_id: new_id("risk"),
        managed_task_ref: managed_task.managed_task_ref.clone(),
        subject_kind: RiskSubjectKind::TaskBaseline,
        capability_snapshot_ref: Some(capability.capability_snapshot_ref.clone()),
        stage_run_ref: managed_task.active_run_ref.clone(),
        decision_area: None,
        overall_risk_band,
        risk_dimensions: dimensions,
        trigger_reason: trigger_reason.into(),
        evidence_refs: vec![
            EvidenceRef {
                kind: "task_scope".into(),
                reference: scope.scope_id.clone(),
            },
            EvidenceRef {
                kind: "capability_snapshot".into(),
                reference: capability.capability_snapshot_ref.clone(),
            },
        ],
        derived_consequences,
        assessed_at: now_timestamp(),
        supersedes,
    }
}

pub fn assess_action_override(
    proposal: &ProposedAction,
    capability: &HostCapabilitySnapshot,
    trigger_reason: impl Into<String>,
) -> RiskAssessment {
    let write_impact = if proposal.writable_roots.iter().any(|root| root == "/") {
        RiskBand::Critical
    } else if proposal.writable_roots.is_empty() {
        RiskBand::Low
    } else {
        RiskBand::High
    };
    let external_side_effect = if proposal.external_side_effect {
        RiskBand::High
    } else {
        RiskBand::Low
    };
    let secret_exposure = if proposal
        .secret_classes
        .iter()
        .any(|class| class.contains("prod") || class.contains("sensitive"))
    {
        RiskBand::Critical
    } else if proposal.secret_classes.is_empty() {
        RiskBand::Low
    } else {
        RiskBand::High
    };
    let irreversibility = if proposal.irreversible {
        RiskBand::Critical
    } else {
        RiskBand::Medium
    };
    let budget_burn = match proposal.estimated_budget_band {
        AuthorizationBudgetBand::Conservative => RiskBand::Low,
        AuthorizationBudgetBand::Standard => RiskBand::Medium,
        AuthorizationBudgetBand::Elevated => RiskBand::High,
    };
    let uncertainty = if proposal.preview_available {
        RiskBand::Medium
    } else {
        RiskBand::High
    };
    let dimensions = RiskDimensions {
        write_impact,
        external_side_effect,
        secret_exposure,
        irreversibility,
        budget_burn,
        uncertainty,
    };
    let overall_risk_band = dimensions.max_band();
    let mut derived_consequences = vec![RiskConsequence::NarrowExecutionAuthorization];
    if overall_risk_band >= RiskBand::High {
        derived_consequences.push(RiskConsequence::RequireApprovalWindow);
        derived_consequences.push(RiskConsequence::RequireAdditionalReview);
    }
    if overall_risk_band == RiskBand::Critical {
        derived_consequences.push(RiskConsequence::StopSilentExecution);
    }
    RiskAssessment {
        assessment_id: new_id("risk"),
        managed_task_ref: proposal.managed_task_ref.clone(),
        subject_kind: RiskSubjectKind::ActionOverride,
        capability_snapshot_ref: Some(capability.capability_snapshot_ref.clone()),
        stage_run_ref: proposal.stage_run_ref.clone(),
        decision_area: Some(match proposal.decision_area {
            DecisionArea::TaskExecution => DecisionArea::TaskExecution,
            DecisionArea::ToolExecution => DecisionArea::ToolExecution,
            DecisionArea::TaskChangeConfirmation => DecisionArea::TaskChangeConfirmation,
            DecisionArea::CapabilityDriftResolution => DecisionArea::CapabilityDriftResolution,
            DecisionArea::ReviewResolution => DecisionArea::ReviewResolution,
        }),
        overall_risk_band,
        risk_dimensions: dimensions,
        trigger_reason: trigger_reason.into(),
        evidence_refs: vec![
            EvidenceRef {
                kind: "proposal".into(),
                reference: proposal.proposal_id.clone(),
            },
            EvidenceRef {
                kind: "capability_snapshot".into(),
                reference: capability.capability_snapshot_ref.clone(),
            },
        ],
        derived_consequences,
        assessed_at: now_timestamp(),
        supersedes: None,
    }
}

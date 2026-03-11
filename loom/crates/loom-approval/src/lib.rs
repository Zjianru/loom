use loom_domain::{
    AuthorizationBudgetBand, AuthorizedDecisionArea, DecisionArea, DelegationBand,
    ExecutionAuthorization, ExecutionAuthorizationStatus, HostCapabilitySnapshot, IsolatedTaskRun,
    ManagedTaskRef, RiskAssessment, RiskBand, RiskConsequence, TaskScopeSnapshot, new_id,
    now_timestamp,
};

fn intersect(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|value| right.contains(value))
        .cloned()
        .collect()
}

pub fn issue_execution_authorization(
    managed_task_ref: ManagedTaskRef,
    run: &IsolatedTaskRun,
    capability: &HostCapabilitySnapshot,
    scope: &TaskScopeSnapshot,
    risk: &RiskAssessment,
    reason: impl Into<String>,
    supersedes: Option<String>,
    narrowed: bool,
) -> ExecutionAuthorization {
    let requires_user_approval = risk.derived_consequences.iter().any(|consequence| {
        matches!(
            consequence,
            RiskConsequence::RequireApprovalWindow | RiskConsequence::StopSilentExecution
        )
    });
    let band = if requires_user_approval || risk.overall_risk_band == RiskBand::Critical {
        DelegationBand::UserMediated
    } else if risk.overall_risk_band == RiskBand::High {
        DelegationBand::Guarded
    } else {
        DelegationBand::Autonomous
    };
    let budget_band = if risk.overall_risk_band == RiskBand::Critical {
        AuthorizationBudgetBand::Conservative
    } else {
        capability.max_budget_band
    };
    ExecutionAuthorization {
        authorization_id: new_id("auth"),
        managed_task_ref,
        run_ref: run.run_ref.clone(),
        stage_run_ref: None,
        capability_snapshot_ref: capability.capability_snapshot_ref.clone(),
        task_scope_ref: scope.scope_id.clone(),
        granted_areas: vec![AuthorizedDecisionArea {
            decision_area: DecisionArea::TaskExecution,
            band,
            allowed_tools: capability.allowed_tools.clone(),
            readable_roots: intersect(&capability.readable_roots, &scope.allowed_roots),
            writable_roots: intersect(&capability.writable_roots, &scope.allowed_roots),
            allowed_secret_classes: intersect(&capability.secret_classes, &scope.secret_classes),
            not_before: None,
            not_after: None,
            budget_band,
            spawn_agent_allowed: capability.supports_spawn_agents,
            requires_user_approval,
        }],
        status: if narrowed {
            ExecutionAuthorizationStatus::Narrowed
        } else {
            ExecutionAuthorizationStatus::Active
        },
        issued_reason: reason.into(),
        issued_at: now_timestamp(),
        expires_at: None,
        supersedes,
    }
}

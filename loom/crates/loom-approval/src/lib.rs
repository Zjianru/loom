use loom_domain::{
    AuthorizationBudgetBand, AuthorizedDecisionArea, AuthorizedSpawnAgentScope,
    AuthorizedSpawnAgentScopeMode, AuthorizedSpawnCapability, DecisionArea, DelegationBand,
    ExecutionAuthorization, ExecutionAuthorizationStatus, HostCapabilityFactSource,
    HostCapabilitySnapshot, HostSessionControlScope, HostSpawnAgentScopeMode, IsolatedTaskRun,
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
    let authorized_spawn_capabilities = issue_authorized_spawn_capabilities(capability);
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
            authorized_spawn_capabilities,
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

fn issue_authorized_spawn_capabilities(
    capability: &HostCapabilitySnapshot,
) -> Vec<AuthorizedSpawnCapability> {
    if capability.session_scope.source != HostCapabilityFactSource::Authoritative
        || capability.session_scope.control_scope != HostSessionControlScope::Children
    {
        return Vec::new();
    }

    capability
        .spawn_capabilities
        .iter()
        .filter(|spawn_capability| spawn_capability.available)
        .filter_map(|spawn_capability| {
            let host_agent_scope = match spawn_capability.host_agent_scope.mode {
                HostSpawnAgentScopeMode::All => AuthorizedSpawnAgentScope {
                    mode: AuthorizedSpawnAgentScopeMode::All,
                    allowed_host_agent_refs: Vec::new(),
                },
                HostSpawnAgentScopeMode::ExplicitList => AuthorizedSpawnAgentScope {
                    mode: AuthorizedSpawnAgentScopeMode::ExplicitList,
                    allowed_host_agent_refs: spawn_capability
                        .host_agent_scope
                        .allowed_host_agent_refs
                        .clone(),
                },
                HostSpawnAgentScopeMode::None | HostSpawnAgentScopeMode::Unknown => {
                    return None;
                }
            };
            Some(AuthorizedSpawnCapability {
                runtime_kind: spawn_capability.runtime_kind,
                host_agent_scope,
                supports_resume_session: spawn_capability.supports_resume_session,
                supports_thread_spawn: spawn_capability.supports_thread_spawn,
                supports_parent_progress_stream: spawn_capability.supports_parent_progress_stream,
            })
        })
        .collect()
}

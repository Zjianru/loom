use loom_domain::{
    AuthorizationBudgetBand, ControlAction, ControlActionKind, ControlActionPayload,
    HostCapabilitySnapshot, HostSubagentLifecycleEnvelope, HostSubagentLifecycleEvent,
    HostSubagentStatus, IngressMeta, InteractionLane, ManagedTaskClass, ProposedAction,
    RequirementItemDraft, RequirementOrigin, RiskConsequence, SemanticDecisionEnvelope,
    SubagentEndedPayload, SubagentSpawnedPayload, TaskActivationReason, WorkHorizonKind, new_id,
    now_timestamp,
};
use loom_harness::{LoomHarness, LoomHarnessError};
use loom_store::LoomStore;
use tempfile::tempdir;

fn test_harness() -> LoomHarness {
    let dir = tempdir().expect("tempdir");
    let runtime_root = dir.keep();
    let store = LoomStore::in_memory(runtime_root).expect("store");
    LoomHarness::new(store)
}

fn capability_snapshot(session: &str) -> HostCapabilitySnapshot {
    HostCapabilitySnapshot {
        capability_snapshot_ref: new_id("cap"),
        host_session_id: session.to_string(),
        allowed_tools: vec!["read_file".into(), "write_file".into(), "git_push".into()],
        readable_roots: vec!["/Users/codez/.openclaw".into(), "/tmp".into()],
        writable_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into(), "dev".into()],
        max_budget_band: AuthorizationBudgetBand::Standard,
        available_agent_ids: vec!["main".into(), "coder".into(), "product_analyst".into()],
        supports_spawn_agents: true,
        supports_pause: true,
        supports_resume: true,
        supports_interrupt: true,
        recorded_at: now_timestamp(),
    }
}

fn managed_candidate(session: &str) -> SemanticDecisionEnvelope {
    SemanticDecisionEnvelope {
        decision_id: new_id("decision"),
        host_session_id: session.to_string(),
        host_message_ref: Some(new_id("host-message")),
        managed_task_ref: None,
        interaction_lane: InteractionLane::ManagedTaskCandidate,
        managed_task_class: Some(ManagedTaskClass::Complex),
        work_horizon: Some(WorkHorizonKind::Improvement),
        task_activation_reason: Some(TaskActivationReason::ExplicitUserRequest),
        title: Some("Refactor first-layer governance".into()),
        summary: Some("Implement the first-layer risk governance chain.".into()),
        expected_outcome: Some("Start candidate becomes execute with scope/risk/auth.".into()),
        requirement_items: vec![RequirementItemDraft {
            text: "Implement scope/risk/auth owner chain".into(),
            origin: RequirementOrigin::InitialDecision,
        }],
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
        allowed_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into()],
        confidence: Some(95),
        created_at: now_timestamp(),
    }
}

#[test]
fn r01_baseline_is_issued_before_first_authorization_after_approve_start() {
    let harness = test_harness();
    harness
        .ingest_capability_snapshot(capability_snapshot("session-r01"))
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("session-r01"))
        .expect("candidate")
        .expect("managed candidate");
    let outbound = harness
        .store()
        .next_outbound(&"session-r01".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card payload");
    };

    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token.clone()),
        })
        .expect("approve_start");

    let events = harness
        .store()
        .list_task_events(&task.managed_task_ref)
        .expect("task events");
    let risk_index = events
        .iter()
        .position(|event| event.event_name == "risk_assessment.created")
        .expect("risk event");
    let auth_index = events
        .iter()
        .position(|event| event.event_name == "execution_authorization.issued")
        .expect("auth event");
    assert!(risk_index < auth_index);

    let task_after = harness
        .store()
        .load_managed_task(&task.managed_task_ref)
        .expect("task")
        .expect("managed task");
    assert_eq!(
        task_after.workflow_stage,
        loom_domain::WorkflowStage::Execute
    );
    assert_eq!(task_after.current_scope_version, Some(1));
    let baseline = harness
        .store()
        .latest_task_baseline(&task.managed_task_ref)
        .expect("baseline")
        .expect("baseline exists");
    assert_eq!(
        baseline.subject_kind,
        loom_domain::RiskSubjectKind::TaskBaseline
    );
    let auth = harness
        .store()
        .latest_execution_authorization(&task.managed_task_ref)
        .expect("auth")
        .expect("auth exists");
    assert_eq!(
        auth.task_scope_ref,
        task_after.current_scope_ref.expect("scope ref")
    );
    let task_execution = auth
        .granted_areas
        .iter()
        .find(|area| area.decision_area == loom_domain::DecisionArea::TaskExecution)
        .expect("task execution area");
    assert!(task_execution.spawn_agent_allowed);
    assert!(task_after.phase_plan.is_some());
    assert!(task_after.agent_binding.is_some());
    let commands = harness
        .store()
        .list_host_execution_commands(&task.managed_task_ref)
        .expect("host execution commands");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].role_kind, loom_domain::AgentRoleKind::Worker);
}

#[test]
fn r02_scope_version_change_supersedes_baseline_and_reissues_authorization() {
    let harness = test_harness();
    harness
        .ingest_capability_snapshot(capability_snapshot("session-r02"))
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("session-r02"))
        .expect("candidate")
        .expect("managed candidate");
    let outbound = harness
        .store()
        .next_outbound(&"session-r02".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card payload");
    };
    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve_start");
    let initial_baseline = harness
        .store()
        .latest_task_baseline(&task.managed_task_ref)
        .expect("baseline")
        .expect("baseline");
    let initial_auth = harness
        .store()
        .latest_execution_authorization(&task.managed_task_ref)
        .expect("auth")
        .expect("auth");

    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::RequestTaskChange,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload {
                summary: Some("Implement first-layer chain and expand writable scope.".into()),
                expected_outcome: Some("Scope version increments and auth reissues.".into()),
                requirement_items: vec![RequirementItemDraft {
                    text: "Add scope change supersede handling".into(),
                    origin: RequirementOrigin::TaskChange,
                }],
                allowed_roots: vec!["/Users/codez/.openclaw".into(), "/tmp".into()],
                secret_classes: vec!["repo".into(), "dev".into()],
                ..ControlActionPayload::default()
            },
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: None,
        })
        .expect("request_task_change");

    let scopes = harness
        .store()
        .list_scope_snapshots(&task.managed_task_ref)
        .expect("scopes");
    assert_eq!(scopes.len(), 2);
    assert_eq!(scopes.last().expect("latest scope").scope_version, 2);
    let new_baseline = harness
        .store()
        .latest_task_baseline(&task.managed_task_ref)
        .expect("baseline")
        .expect("baseline exists");
    assert_eq!(
        new_baseline.supersedes,
        Some(initial_baseline.assessment_id)
    );
    let new_auth = harness
        .store()
        .latest_execution_authorization(&task.managed_task_ref)
        .expect("auth")
        .expect("auth exists");
    assert_eq!(new_auth.supersedes, Some(initial_auth.authorization_id));
}

#[test]
fn r03_risky_action_creates_action_override_without_replacing_baseline() {
    let harness = test_harness();
    harness
        .ingest_capability_snapshot(capability_snapshot("session-r03"))
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("session-r03"))
        .expect("candidate")
        .expect("managed candidate");
    let outbound = harness
        .store()
        .next_outbound(&"session-r03".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card");
    };
    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve_start");
    let baseline = harness
        .store()
        .latest_task_baseline(&task.managed_task_ref)
        .expect("baseline")
        .expect("baseline");

    let override_assessment = harness
        .evaluate_proposed_action(ProposedAction {
            proposal_id: new_id("proposal"),
            managed_task_ref: task.managed_task_ref.clone(),
            stage_run_ref: None,
            decision_area: loom_domain::DecisionArea::ToolExecution,
            tool_name: Some("git_push".into()),
            readable_roots: vec!["/Users/codez/.openclaw".into()],
            writable_roots: vec!["/Users/codez/.openclaw".into()],
            secret_classes: vec!["repo".into()],
            external_side_effect: true,
            irreversible: false,
            estimated_budget_band: AuthorizationBudgetBand::Standard,
            preview_available: false,
            reason: "Push changes to remote".into(),
        })
        .expect("override");

    assert_eq!(
        override_assessment.subject_kind,
        loom_domain::RiskSubjectKind::ActionOverride
    );
    assert!(
        override_assessment
            .derived_consequences
            .contains(&RiskConsequence::NarrowExecutionAuthorization)
    );
    let latest_baseline = harness
        .store()
        .latest_task_baseline(&task.managed_task_ref)
        .expect("baseline")
        .expect("baseline exists");
    assert_eq!(latest_baseline.assessment_id, baseline.assessment_id);
}

#[test]
fn r04_worker_and_recorder_lifecycle_create_review_and_result_chain() {
    let harness = test_harness();
    harness
        .ingest_capability_snapshot(capability_snapshot("session-r04"))
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("session-r04"))
        .expect("candidate")
        .expect("managed candidate");
    let outbound = harness
        .store()
        .next_outbound(&"session-r04".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card");
    };
    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve_start");

    let worker_command = harness
        .store()
        .list_host_execution_commands(&task.managed_task_ref)
        .expect("commands")
        .into_iter()
        .find(|command| command.role_kind == loom_domain::AgentRoleKind::Worker)
        .expect("worker command");
    harness
        .store()
        .ack_host_execution_command(&worker_command.command_id)
        .expect("ack worker command");
    harness
        .ingest_subagent_lifecycle(HostSubagentLifecycleEnvelope {
            meta: IngressMeta::default(),
            command_id: worker_command.command_id.clone(),
            managed_task_ref: task.managed_task_ref.clone(),
            run_ref: worker_command.run_ref.clone(),
            role_kind: loom_domain::AgentRoleKind::Worker,
            event: HostSubagentLifecycleEvent::Spawned(SubagentSpawnedPayload {
                host_child_execution_ref: "agent:coder:child-1".into(),
                host_child_run_ref: Some("run-child-1".into()),
                host_agent_id: "coder".into(),
                observed_at: now_timestamp(),
            }),
        })
        .expect("worker spawned");
    harness
        .ingest_subagent_lifecycle(HostSubagentLifecycleEnvelope {
            meta: IngressMeta::default(),
            command_id: worker_command.command_id.clone(),
            managed_task_ref: task.managed_task_ref.clone(),
            run_ref: worker_command.run_ref.clone(),
            role_kind: loom_domain::AgentRoleKind::Worker,
            event: HostSubagentLifecycleEvent::Ended(SubagentEndedPayload {
                host_child_execution_ref: "agent:coder:child-1".into(),
                host_child_run_ref: Some("run-child-1".into()),
                host_agent_id: "coder".into(),
                status: HostSubagentStatus::Completed,
                output_summary: "Summary: implemented the scope.\nChanged files: loom/README.md\nVerification: cargo test -p loom-harness".into(),
                artifact_refs: vec!["loom/README.md".into()],
                observed_at: now_timestamp(),
            }),
        })
        .expect("worker ended");

    let review = harness
        .store()
        .latest_review_result(&task.managed_task_ref)
        .expect("review")
        .expect("review exists");
    assert_eq!(review.review_verdict, loom_domain::ReviewVerdict::Approved);

    let recorder_command = harness
        .store()
        .list_host_execution_commands(&task.managed_task_ref)
        .expect("commands")
        .into_iter()
        .find(|command| command.role_kind == loom_domain::AgentRoleKind::Recorder)
        .expect("recorder command");
    harness
        .store()
        .ack_host_execution_command(&recorder_command.command_id)
        .expect("ack recorder command");
    harness
        .ingest_subagent_lifecycle(HostSubagentLifecycleEnvelope {
            meta: IngressMeta::default(),
            command_id: recorder_command.command_id.clone(),
            managed_task_ref: task.managed_task_ref.clone(),
            run_ref: recorder_command.run_ref.clone(),
            role_kind: loom_domain::AgentRoleKind::Recorder,
            event: HostSubagentLifecycleEvent::Ended(SubagentEndedPayload {
                host_child_execution_ref: "agent:product_analyst:child-2".into(),
                host_child_run_ref: Some("run-child-2".into()),
                host_agent_id: "product_analyst".into(),
                status: HostSubagentStatus::Completed,
                output_summary: "Summary: first-round closed loop completed.\nKey outcomes: review approved.\nProof excerpt: review + artifacts captured.\nNext actions: verify the WebUI summary.".into(),
                artifact_refs: vec!["loom/README.md".into()],
                observed_at: now_timestamp(),
            }),
        })
        .expect("recorder ended");

    let task_after = harness
        .store()
        .load_managed_task(&task.managed_task_ref)
        .expect("task")
        .expect("managed task");
    assert_eq!(
        task_after.workflow_stage,
        loom_domain::WorkflowStage::Result
    );
    assert!(task_after.proof_of_work_bundle.is_some());
    assert!(task_after.result_contract.is_some());
    let result_contract = harness
        .store()
        .latest_result_contract(&task.managed_task_ref)
        .expect("result contract")
        .expect("result contract exists");
    assert_eq!(
        result_contract.outcome,
        loom_domain::ResultOutcome::Completed
    );
    let proof = harness
        .store()
        .latest_proof_of_work_bundle(&task.managed_task_ref)
        .expect("proof")
        .expect("proof exists");
    assert_eq!(
        proof.acceptance_verdict,
        loom_domain::AcceptanceResult::Accepted
    );
}

#[test]
fn r04_critical_risk_stops_silent_execution_and_opens_approval_window() {
    let harness = test_harness();
    let mut capability = capability_snapshot("session-r04");
    capability.secret_classes.push("prod".into());
    harness
        .ingest_capability_snapshot(capability)
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("session-r04"))
        .expect("candidate")
        .expect("managed candidate");
    let outbound = harness
        .store()
        .next_outbound(&"session-r04".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card");
    };
    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve_start");

    let override_assessment = harness
        .evaluate_proposed_action(ProposedAction {
            proposal_id: new_id("proposal"),
            managed_task_ref: task.managed_task_ref.clone(),
            stage_run_ref: None,
            decision_area: loom_domain::DecisionArea::ToolExecution,
            tool_name: Some("git_push".into()),
            readable_roots: vec!["/Users/codez/.openclaw".into()],
            writable_roots: vec!["/".into()],
            secret_classes: vec!["prod".into()],
            external_side_effect: true,
            irreversible: true,
            estimated_budget_band: AuthorizationBudgetBand::Elevated,
            preview_available: false,
            reason: "Push irreversible production change".into(),
        })
        .expect("override");

    assert_eq!(
        override_assessment.overall_risk_band,
        loom_domain::RiskBand::Critical
    );
    assert!(
        override_assessment
            .derived_consequences
            .contains(&RiskConsequence::StopSilentExecution)
    );
    let task_after = harness
        .store()
        .load_managed_task(&task.managed_task_ref)
        .expect("task")
        .expect("task");
    let pending_window = task_after
        .current_pending_window_ref
        .expect("approval window");
    let window = harness
        .store()
        .load_pending_decision_window(&pending_window)
        .expect("window")
        .expect("window exists");
    assert_eq!(
        window.kind,
        loom_domain::PendingDecisionWindowKind::ApprovalRequest
    );
}

#[test]
fn r05_capability_drift_reissues_authorization_against_new_snapshot() {
    let harness = test_harness();
    harness
        .ingest_capability_snapshot(capability_snapshot("session-r05"))
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("session-r05"))
        .expect("candidate")
        .expect("managed candidate");
    let outbound = harness
        .store()
        .next_outbound(&"session-r05".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card");
    };
    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve_start");
    let initial_auth = harness
        .store()
        .latest_execution_authorization(&task.managed_task_ref)
        .expect("auth")
        .expect("auth exists");

    let mut drifted = capability_snapshot("session-r05");
    drifted.allowed_tools = vec!["read_file".into()];
    drifted.writable_roots = vec![];
    drifted.capability_snapshot_ref = new_id("cap");
    harness
        .ingest_capability_snapshot(drifted.clone())
        .expect("drifted capability");

    let reissued = harness
        .store()
        .latest_execution_authorization(&task.managed_task_ref)
        .expect("auth")
        .expect("auth exists");
    assert_ne!(reissued.authorization_id, initial_auth.authorization_id);
    assert_eq!(reissued.supersedes, Some(initial_auth.authorization_id));
    assert_eq!(
        reissued.capability_snapshot_ref,
        drifted.capability_snapshot_ref
    );
}

#[test]
fn bridge_contract_missing_decision_token_fails_closed() {
    let harness = test_harness();
    let error = harness.ingest_control_action(ControlAction {
        action_id: new_id("action"),
        managed_task_ref: Some(new_id("task")),
        kind: ControlActionKind::ApproveStart,
        actor: loom_domain::ControlActorRef::User,
        payload: ControlActionPayload::default(),
        source_decision_ref: None,
        decision_token: None,
    });
    assert!(matches!(
        error
            .expect_err("missing token")
            .downcast_ref::<LoomHarnessError>(),
        Some(LoomHarnessError::MissingDecisionToken)
    ));
}

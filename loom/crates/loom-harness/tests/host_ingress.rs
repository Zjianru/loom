use loom_domain::{
    AuthorizationBudgetBand, ControlAction, ControlActionKind, ControlActionPayload,
    CurrentTurnEnvelope, HostCapabilitySnapshot, IngressMeta, InteractionLane, ManagedTaskClass,
    RequirementItemDraft, RequirementOrigin, SemanticDecisionEnvelope, TaskActivationReason,
    WorkHorizonKind, new_id, now_timestamp,
};
use loom_harness::LoomHarness;
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
        allowed_tools: vec!["read_file".into(), "write_file".into()],
        readable_roots: vec!["/Users/codez/.openclaw".into()],
        writable_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into()],
        max_budget_band: AuthorizationBudgetBand::Standard,
        available_agent_ids: vec!["main".into(), "coder".into(), "product_analyst".into()],
        supports_spawn_agents: true,
        supports_pause: true,
        supports_resume: true,
        supports_interrupt: true,
        recorded_at: now_timestamp(),
    }
}

fn capability_snapshot_with_agents(
    session: &str,
    available_agent_ids: Vec<&str>,
    supports_spawn_agents: bool,
) -> HostCapabilitySnapshot {
    HostCapabilitySnapshot {
        available_agent_ids: available_agent_ids.into_iter().map(str::to_string).collect(),
        supports_spawn_agents,
        ..capability_snapshot(session)
    }
}

fn current_turn(session: &str, ingress_id: &str) -> CurrentTurnEnvelope {
    CurrentTurnEnvelope {
        meta: IngressMeta {
            ingress_id: ingress_id.into(),
            received_at: now_timestamp(),
            causation_id: None,
            correlation_id: "corr-current-turn".into(),
            dedupe_window: "PT10M".into(),
        },
        host_session_id: session.to_string(),
        host_message_ref: Some("host-message-1".into()),
        text: "Please start the Loom bridge task".into(),
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
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
        title: Some("Bridge task".into()),
        summary: Some("Create a managed task candidate.".into()),
        expected_outcome: Some("Task enters execute after approval.".into()),
        requirement_items: vec![RequirementItemDraft {
            text: "Start from current turn".into(),
            origin: RequirementOrigin::InitialDecision,
        }],
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
        allowed_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into()],
        confidence: Some(97),
        created_at: now_timestamp(),
    }
}

#[test]
fn current_turn_ingress_is_deduplicated_by_ingress_id_and_window() {
    let harness = test_harness();
    let turn = current_turn("session-current-turn", "ingress-1");

    harness
        .ingest_current_turn(turn.clone())
        .expect("first current turn");
    harness
        .ingest_current_turn(turn)
        .expect("duplicate current turn");

    let latest = harness
        .store()
        .latest_current_turn(&"session-current-turn".to_string())
        .expect("latest current turn")
        .expect("turn exists");
    assert_eq!(latest.host_message_ref.as_deref(), Some("host-message-1"));
    assert_eq!(latest.meta.correlation_id, "corr-current-turn");

    let receipt_count = harness
        .store()
        .count_ingress_receipts("current_turn")
        .expect("receipt count");
    assert_eq!(receipt_count, 1);
}

#[test]
fn managed_task_active_lane_does_not_open_a_new_candidate() {
    let harness = test_harness();
    harness
        .ingest_current_turn(current_turn("session-active", "ingress-active-1"))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot("session-active"))
        .expect("capability");

    let task = harness
        .ingest_semantic_decision(managed_candidate("session-active"))
        .expect("candidate")
        .expect("managed task");
    let start_card = harness
        .store()
        .next_outbound(&"session-active".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = start_card.payload else {
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

    let active_lane = SemanticDecisionEnvelope {
        decision_id: new_id("decision"),
        host_session_id: "session-active".into(),
        host_message_ref: Some("host-message-2".into()),
        managed_task_ref: Some(task.managed_task_ref.clone()),
        interaction_lane: InteractionLane::ManagedTaskActive,
        managed_task_class: Some(ManagedTaskClass::Complex),
        work_horizon: Some(WorkHorizonKind::Improvement),
        task_activation_reason: Some(TaskActivationReason::ExplicitUserRequest),
        title: Some("Bridge task".into()),
        summary: Some("Continue the active task.".into()),
        expected_outcome: Some("Do not create a new candidate.".into()),
        requirement_items: Vec::new(),
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
        allowed_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into()],
        confidence: Some(94),
        created_at: now_timestamp(),
    };
    let result = harness
        .ingest_semantic_decision(active_lane)
        .expect("active lane decision");
    assert!(result.is_none());

    let maybe_outbound = harness
        .store()
        .next_outbound(&"session-active".to_string())
        .expect("outbound after active lane");
    let Some(outbound) = maybe_outbound else {
        panic!("expected execute feedback");
    };
    let loom_domain::KernelOutboundPayload::ToolDecision(tool_decision) = outbound.payload else {
        panic!("expected tool decision feedback");
    };
    assert_eq!(tool_decision.managed_task_ref, task.managed_task_ref);
    assert_eq!(tool_decision.decision_area, loom_domain::DecisionArea::TaskExecution);
}

#[test]
fn approve_start_blocks_when_capability_snapshot_lacks_required_recorder_roundtrip() {
    let harness = test_harness();
    harness
        .ingest_current_turn(current_turn("session-blocked", "ingress-blocked-1"))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot_with_agents(
            "session-blocked",
            vec!["coder"],
            true,
        ))
        .expect("capability");

    let task = harness
        .ingest_semantic_decision(managed_candidate("session-blocked"))
        .expect("candidate")
        .expect("managed task");
    let start_card = harness
        .store()
        .next_outbound(&"session-blocked".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = start_card.payload else {
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

    let result = harness
        .store()
        .latest_result_contract(&task.managed_task_ref)
        .expect("result")
        .expect("result exists");
    assert_eq!(result.outcome, loom_domain::ResultOutcome::Blocked);
}

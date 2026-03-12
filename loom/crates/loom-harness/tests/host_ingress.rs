use loom_domain::{
    AuthorizationBudgetBand, BoundaryRecommendation, ChangeExecutionSurface, ControlAction,
    ControlActionKind, ControlActionPayload, CurrentTurnEnvelope, DecisionSource,
    HostCapabilitySnapshot, IngressMeta, InteractionLane, InteractionLaneDecisionPayload,
    LegacySemanticDecisionEnvelope as SemanticDecisionEnvelope, ManagedTaskClass,
    RequirementItemDraft, RequirementOrigin, SemanticDecisionBatchEnvelope,
    SemanticDecisionEnvelope as StoredSemanticDecision, SemanticDecisionKind,
    SemanticDecisionPayload, TaskActivationReason, TaskChangeClassification,
    TaskChangeDecisionPayload, WorkHorizonKind, new_id, now_timestamp,
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
        available_agent_ids: available_agent_ids
            .into_iter()
            .map(str::to_string)
            .collect(),
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
        task_activation_reason: Some(TaskActivationReason::ExplicitStartTask),
        task_change_classification: None,
        task_change_execution_surface: None,
        task_change_boundary_recommendation: None,
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

fn activate_task(harness: &LoomHarness, session: &str, ingress_id: &str) -> loom_domain::ManagedTask {
    harness
        .ingest_current_turn(current_turn(session, ingress_id))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot(session))
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(managed_candidate(session))
        .expect("candidate")
        .expect("managed task");
    let start_card = harness
        .store()
        .next_outbound(&session.to_string())
        .expect("start card outbound")
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
    task
}

fn paired_request_task_change_batch(
    session: &str,
    managed_task_ref: &str,
    ingress_id: &str,
    source_decision_ref: &str,
) -> SemanticDecisionBatchEnvelope {
    SemanticDecisionBatchEnvelope {
        meta: IngressMeta {
            ingress_id: ingress_id.into(),
            received_at: now_timestamp(),
            causation_id: None,
            correlation_id: format!("corr-{ingress_id}"),
            dedupe_window: "PT10M".into(),
        },
        host_session_id: session.into(),
        host_message_ref: Some(format!("host-message-{ingress_id}")),
        input_ref: format!("input-{ingress_id}"),
        source_model_ref: "host-model".into(),
        issued_at: now_timestamp(),
        rationale_summary: Some("paired request_task_change batch".into()),
        semantic_decisions: vec![
            StoredSemanticDecision {
                decision_ref: format!("decision-interaction-{ingress_id}"),
                host_session_id: session.into(),
                host_message_ref: Some(format!("host-message-{ingress_id}")),
                managed_task_ref: Some(managed_task_ref.into()),
                decision_kind: SemanticDecisionKind::InteractionLane,
                decision_source: DecisionSource::HostModel,
                rationale: "the turn targets the active task".into(),
                confidence: 95,
                source_model_ref: "host-model".into(),
                issued_at: now_timestamp(),
                decision_payload: SemanticDecisionPayload::InteractionLane(
                    InteractionLaneDecisionPayload {
                        interaction_lane: InteractionLane::ManagedTaskActive,
                        managed_task_ref: Some(managed_task_ref.into()),
                        title: None,
                        summary: Some("Update the active task".into()),
                        expected_outcome: None,
                        requirement_items: Vec::new(),
                        workspace_ref: None,
                        repo_ref: None,
                        allowed_roots: Vec::new(),
                        secret_classes: Vec::new(),
                    },
                ),
            },
            StoredSemanticDecision {
                decision_ref: source_decision_ref.into(),
                host_session_id: session.into(),
                host_message_ref: Some(format!("host-message-{ingress_id}")),
                managed_task_ref: Some(managed_task_ref.into()),
                decision_kind: SemanticDecisionKind::TaskChange,
                decision_source: DecisionSource::HostModel,
                rationale: "future-only same-task change".into(),
                confidence: 88,
                source_model_ref: "host-model".into(),
                issued_at: now_timestamp(),
                decision_payload: SemanticDecisionPayload::TaskChange(
                    TaskChangeDecisionPayload {
                        classification: TaskChangeClassification::SameTaskMinor,
                        execution_surface: ChangeExecutionSurface::FutureOnly,
                        boundary_recommendation: BoundaryRecommendation::AbsorbChange,
                    },
                ),
            },
        ],
        control_action: Some(loom_domain::ControlActionEnvelope {
            decision_ref: format!("decision-control-{ingress_id}"),
            decision_source: DecisionSource::UserControlAction,
            rationale: "explicit request to change the active task".into(),
            confidence: 99,
            source_model_ref: "host-model".into(),
            issued_at: now_timestamp(),
            action: ControlAction {
                action_id: format!("action-{ingress_id}"),
                managed_task_ref: Some(managed_task_ref.into()),
                kind: ControlActionKind::RequestTaskChange,
                actor: loom_domain::ControlActorRef::User,
                payload: ControlActionPayload {
                    summary: Some("Expand the task into the notifier workspace.".into()),
                    expected_outcome: Some(
                        "Task context and scope both point at notifier.".into(),
                    ),
                    workspace_ref: Some("/Users/codez/.openclaw/notifier".into()),
                    repo_ref: Some("openclaw-notifier".into()),
                    allowed_roots: vec![
                        "/Users/codez/.openclaw".into(),
                        "/Users/codez/.openclaw/notifier".into(),
                    ],
                    secret_classes: vec!["repo".into()],
                    ..ControlActionPayload::default()
                },
                source_decision_ref: Some(source_decision_ref.into()),
                decision_token: None,
            },
        }),
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
        task_activation_reason: Some(TaskActivationReason::ExplicitStartTask),
        task_change_classification: None,
        task_change_execution_surface: None,
        task_change_boundary_recommendation: None,
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
    let loom_domain::KernelOutboundPayload::StatusNotice(status_notice) = outbound.payload else {
        panic!("expected stage-entered status notice");
    };
    assert_eq!(status_notice.managed_task_ref, task.managed_task_ref);
    assert_eq!(
        status_notice.notice_kind,
        loom_domain::StatusNoticeKind::StageEntered
    );
    assert_eq!(status_notice.headline, "Entered execute stage");
    assert!(status_notice.summary.contains("queued worker dispatch"));
    assert!(!status_notice.stage_ref.is_empty());
}

#[test]
fn approve_start_uses_candidate_window_source_decision_for_initial_scope() {
    let harness = test_harness();
    let session = "session-start-source";
    harness
        .ingest_current_turn(current_turn(session, "ingress-start-source-1"))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot(session))
        .expect("capability");

    let candidate = managed_candidate(session);
    let candidate_decision_ref = candidate.decision_id.clone();
    let task = harness
        .ingest_semantic_decision(candidate)
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&session.to_string())
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
            source_decision_ref: None,
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve_start");

    let scope = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("scope query")
        .expect("scope exists");
    assert_eq!(scope.source_decision_ref, candidate_decision_ref);
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

    let blocked_notice = harness
        .store()
        .next_outbound(&"session-blocked".to_string())
        .expect("blocked notice outbound")
        .expect("blocked notice exists");
    let loom_domain::KernelOutboundPayload::StatusNotice(status_notice) = blocked_notice.payload
    else {
        panic!("expected blocked status notice");
    };
    assert_eq!(status_notice.managed_task_ref, task.managed_task_ref);
    assert_eq!(
        status_notice.notice_kind,
        loom_domain::StatusNoticeKind::Blocked
    );
    assert_eq!(status_notice.headline, "Execute stage blocked");
    assert!(
        status_notice
            .summary
            .contains("cannot spawn the required worker/recorder agents")
    );
    assert!(!status_notice.stage_ref.is_empty());

    let result_outbound = harness
        .store()
        .next_outbound(&"session-blocked".to_string())
        .expect("result outbound")
        .expect("result summary exists");
    let loom_domain::KernelOutboundPayload::ResultSummary(result_summary) = result_outbound.payload
    else {
        panic!("expected blocked result summary");
    };
    assert_eq!(result_summary.managed_task_ref, task.managed_task_ref);
    assert_eq!(result_summary.outcome, loom_domain::ResultOutcome::Blocked);

    let result = harness
        .store()
        .latest_result_contract(&task.managed_task_ref)
        .expect("result")
        .expect("result exists");
    assert_eq!(result.outcome, loom_domain::ResultOutcome::Blocked);
}

#[test]
fn request_task_change_without_source_judgment_requests_clarification_without_mutating_scope() {
    let harness = test_harness();
    harness
        .ingest_current_turn(current_turn("session-task-change", "ingress-task-change-1"))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot("session-task-change"))
        .expect("capability");

    let task = harness
        .ingest_semantic_decision(managed_candidate("session-task-change"))
        .expect("candidate")
        .expect("managed task");
    let start_card = harness
        .store()
        .next_outbound(&"session-task-change".to_string())
        .expect("start card outbound")
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

    let stage_notice = harness
        .store()
        .next_outbound(&"session-task-change".to_string())
        .expect("stage notice outbound")
        .expect("stage notice");
    harness
        .store()
        .ack_outbound(&stage_notice.delivery_id)
        .expect("ack stage notice");

    let scope_count_before = harness
        .store()
        .list_scope_snapshots(&task.managed_task_ref)
        .expect("scope snapshots before")
        .len();

    let error = harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::RequestTaskChange,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload {
                summary: Some("Expand the active task.".into()),
                expected_outcome: Some("Task covers the notifier workspace.".into()),
                workspace_ref: Some("/Users/codez/.openclaw/notifier".into()),
                repo_ref: Some("openclaw-notifier".into()),
                ..ControlActionPayload::default()
            },
            source_decision_ref: None,
            decision_token: None,
        })
        .expect_err("missing source decision ref should fail");
    assert!(
        error
            .to_string()
            .contains("request_task_change requires source_decision_ref")
    );

    let scope_count_after = harness
        .store()
        .list_scope_snapshots(&task.managed_task_ref)
        .expect("scope snapshots after")
        .len();
    assert_eq!(scope_count_before, 1);
    assert_eq!(scope_count_after, 1);

    let events = harness
        .store()
        .list_task_events(&task.managed_task_ref)
        .expect("task events");
    assert!(events.iter().any(|event| {
        event.event_name == "task_change_clarification_requested"
    }));
    assert!(!events.iter().any(|event| {
        event.event_name == "task_change_requested"
    }));

    let blocked_notice = harness
        .store()
        .next_outbound(&"session-task-change".to_string())
        .expect("clarification notice outbound")
        .expect("clarification notice");
    let loom_domain::KernelOutboundPayload::StatusNotice(blocked_notice) = blocked_notice.payload
    else {
        panic!("expected blocked status notice");
    };
    assert_eq!(blocked_notice.headline, "Task change needs clarification");
    assert_eq!(
        blocked_notice.notice_kind,
        loom_domain::StatusNoticeKind::Blocked
    );
    assert!(
        blocked_notice
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("what should change")
    );
}

#[test]
fn semantic_bundle_rolls_back_authoritative_rows_when_transaction_fails_mid_commit() {
    let harness = test_harness();
    let task = activate_task(&harness, "session-batch-tx-rollback", "ingress-batch-tx-rollback-1");
    harness.store().inject_failpoint("tx.save_scope_snapshot");

    let source_decision_ref = "decision-task-change-tx-rollback";
    let error = harness
        .ingest_semantic_bundle(paired_request_task_change_batch(
            "session-batch-tx-rollback",
            &task.managed_task_ref,
            "ingress-batch-tx-rollback-2",
            source_decision_ref,
        ))
        .expect_err("mid-transaction failure should roll back");
    assert!(error.to_string().contains("tx.save_scope_snapshot"));

    assert_eq!(
        harness
            .store()
            .semantic_decision_batch_status("ingress-batch-tx-rollback-2")
            .expect("batch status"),
        None
    );
    assert!(
        harness
            .store()
            .load_semantic_decision(source_decision_ref)
            .expect("semantic decision lookup")
            .is_none()
    );
    let latest_scope = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("latest scope")
        .expect("scope exists");
    assert_eq!(latest_scope.scope_version, 1);
    let events = harness
        .store()
        .list_task_events(&task.managed_task_ref)
        .expect("task events");
    assert!(!events.iter().any(|event| event.event_name == "task_change_requested"));
}

#[test]
fn semantic_bundle_projection_failure_keeps_authoritative_commit_and_records_audit() {
    let harness = test_harness();
    let task = activate_task(
        &harness,
        "session-batch-projection-failure",
        "ingress-batch-projection-failure-1",
    );
    harness.store().inject_failpoint("projection.write_json");

    let source_decision_ref = "decision-task-change-projection-failure";
    harness
        .ingest_semantic_bundle(paired_request_task_change_batch(
            "session-batch-projection-failure",
            &task.managed_task_ref,
            "ingress-batch-projection-failure-2",
            source_decision_ref,
        ))
        .expect("projection failure should not roll back authoritative commit");

    assert_eq!(
        harness
            .store()
            .semantic_decision_batch_status("ingress-batch-projection-failure-2")
            .expect("batch status")
            .as_deref(),
        Some("accepted")
    );
    assert!(
        harness
            .store()
            .load_semantic_decision(source_decision_ref)
            .expect("semantic decision lookup")
            .is_some()
    );
    let latest_scope = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("latest scope")
        .expect("scope exists");
    assert_eq!(latest_scope.scope_version, 2);
    assert_eq!(
        harness
            .store()
            .count_projection_failures()
            .expect("projection failure count"),
        1
    );
}

#[test]
fn approve_start_legacy_candidate_without_source_decision_ref_uses_compat_source_ref() {
    let harness = test_harness();
    harness
        .ingest_current_turn(current_turn("legacy-approve-session", "ingress-legacy-approve-turn"))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot("legacy-approve-session"))
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(managed_candidate("legacy-approve-session"))
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&"legacy-approve-session".to_string())
        .expect("outbound")
        .expect("start card");
    let loom_domain::KernelOutboundPayload::StartCard(start_card) = outbound.payload else {
        panic!("expected start card");
    };
    let mut window = harness
        .store()
        .find_open_window_by_token(&start_card.decision_token)
        .expect("open window lookup")
        .expect("open window");
    window.source_decision_ref = None;
    harness
        .store()
        .save_pending_decision_window(&window)
        .expect("save legacy window");

    harness
        .ingest_control_action(ControlAction {
            action_id: new_id("action"),
            managed_task_ref: Some(task.managed_task_ref.clone()),
            kind: ControlActionKind::ApproveStart,
            actor: loom_domain::ControlActorRef::User,
            payload: ControlActionPayload::default(),
            source_decision_ref: None,
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve legacy candidate");

    let latest_scope = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("latest scope")
        .expect("scope exists");
    assert_eq!(
        latest_scope.source_decision_ref,
        format!("legacy-candidate-window:{}", window.window_id)
    );
}

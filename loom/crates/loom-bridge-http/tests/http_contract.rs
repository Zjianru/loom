use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use loom_bridge_http::{BOOTSTRAP_TICKET_RELATIVE_PATH, build_router};
use loom_domain::{
    AuthorizationBudgetBand, BridgeBootstrapAck, BridgeBootstrapMaterial, BridgeHealthResponse,
    ControlAction, ControlActionKind, ControlActionPayload, CurrentTurnEnvelope, DeliveryStatus,
    HostCapabilitySnapshot, IngressMeta, InteractionLane,
    LegacySemanticDecisionEnvelope as SemanticDecisionEnvelope, ManagedTaskClass,
    OutboundDelivery, RequirementItemDraft, RequirementOrigin, TaskActivationReason,
    WorkHorizonKind, new_id, now_timestamp,
};
use loom_harness::LoomHarness;
use loom_store::LoomStore;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::read_to_string;
use std::path::Path;
use tempfile::tempdir;
use tower::ServiceExt;

const ADAPTER_ID: &str = "loom-openclaw";

fn harness() -> LoomHarness {
    let dir = tempdir().expect("tempdir");
    let store = LoomStore::in_memory(dir.keep()).expect("store");
    LoomHarness::new(store)
}

fn capability_snapshot(session: &str) -> HostCapabilitySnapshot {
    HostCapabilitySnapshot {
        capability_snapshot_ref: new_id("cap"),
        host_session_id: session.into(),
        allowed_tools: vec!["read_file".into()],
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

fn managed_candidate(session: &str) -> SemanticDecisionEnvelope {
    SemanticDecisionEnvelope {
        decision_id: new_id("decision"),
        host_session_id: session.into(),
        host_message_ref: Some(new_id("host-message")),
        managed_task_ref: None,
        interaction_lane: InteractionLane::ManagedTaskCandidate,
        managed_task_class: Some(ManagedTaskClass::Complex),
        work_horizon: Some(WorkHorizonKind::Improvement),
        task_activation_reason: Some(TaskActivationReason::ExplicitStartTask),
        task_change_classification: None,
        task_change_execution_surface: None,
        task_change_boundary_recommendation: None,
        title: Some("Bridge test".into()),
        summary: Some("Bridge candidate".into()),
        expected_outcome: Some("Bridge start card".into()),
        requirement_items: vec![RequirementItemDraft {
            text: "Create start card via HTTP".into(),
            origin: RequirementOrigin::InitialDecision,
        }],
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
        allowed_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into()],
        confidence: Some(90),
        created_at: now_timestamp(),
    }
}

fn current_turn(session: &str) -> loom_domain::CurrentTurnEnvelope {
    loom_domain::CurrentTurnEnvelope {
        meta: IngressMeta {
            ingress_id: "ingress-1".into(),
            received_at: now_timestamp(),
            causation_id: None,
            correlation_id: "corr-1".into(),
            dedupe_window: "PT10M".into(),
        },
        host_session_id: session.into(),
        host_message_ref: Some("host-message-1".into()),
        text: "Please start a managed task".into(),
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
    }
}

fn read_bridge_audits(runtime_root: &Path) -> Vec<Value> {
    read_to_string(runtime_root.join("host-bridges/openclaw/bridge-auth/audit.jsonl"))
        .expect("audit jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("audit line"))
        .collect()
}

fn bootstrap_material(runtime_root: &Path) -> BridgeBootstrapMaterial {
    let payload = read_to_string(runtime_root.join(BOOTSTRAP_TICKET_RELATIVE_PATH))
        .expect("bootstrap ticket file");
    serde_json::from_str(&payload).expect("bootstrap material")
}

fn sign_headers(
    method: &str,
    path: &str,
    body: &[u8],
    bridge_instance_id: &str,
    secret_ref: &str,
    rotation_epoch: u32,
    session_secret: &str,
) -> Vec<(&'static str, String)> {
    let signed_at = now_timestamp();
    let nonce = new_id("nonce");
    let body_sha = format!("{:x}", Sha256::digest(body));
    let canonical = [method, path, &body_sha, &signed_at, &nonce].join("\n");
    let mut mac =
        hmac::Hmac::<Sha256>::new_from_slice(session_secret.as_bytes()).expect("session secret");
    use hmac::Mac;
    mac.update(canonical.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    vec![
        ("x-loom-bridge-instance-id", bridge_instance_id.to_string()),
        ("x-loom-adapter-id", ADAPTER_ID.to_string()),
        ("x-loom-secret-ref", secret_ref.to_string()),
        ("x-loom-rotation-epoch", rotation_epoch.to_string()),
        ("x-loom-signed-at", signed_at),
        ("x-loom-nonce", nonce),
        ("x-loom-signature", signature),
    ]
}

async fn bootstrap(app: axum::Router, runtime_root: &Path) -> BridgeBootstrapAck {
    let material = bootstrap_material(runtime_root);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&loom_domain::BridgeBootstrapRequest {
                        bridge_instance_id: material.bridge_instance_id.clone(),
                        adapter_id: ADAPTER_ID.into(),
                        ticket_id: material.ticket_id.clone(),
                        ticket_secret: material.ticket_secret.clone(),
                        requested_at: now_timestamp(),
                    })
                    .expect("json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("bootstrap ack")
}

#[tokio::test]
async fn health_and_bootstrap_issue_bridge_session_credentials() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/v1/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let health: BridgeHealthResponse = serde_json::from_slice(&bytes).expect("health");
    assert_eq!(health.status, "ready");

    let material = bootstrap_material(&runtime_root);
    assert_eq!(material.bridge_instance_id, health.bridge_instance_id);

    let ack = bootstrap(app, &runtime_root).await;
    assert_eq!(ack.bridge_instance_id, health.bridge_instance_id);
    assert_eq!(ack.rotation_epoch, 1);
    assert!(!ack.session_secret.is_empty());
}

#[tokio::test]
async fn bootstrap_rotates_ticket_for_reconnect_and_revokes_old_credentials() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness);

    let first_material = bootstrap_material(&runtime_root);
    let first_ack = bootstrap(app.clone(), &runtime_root).await;
    let rotated_material = bootstrap_material(&runtime_root);

    assert_eq!(
        rotated_material.bridge_instance_id,
        first_material.bridge_instance_id
    );
    assert_ne!(rotated_material.ticket_id, first_material.ticket_id);
    assert_ne!(rotated_material.ticket_secret, first_material.ticket_secret);

    let second_ack = bootstrap(app.clone(), &runtime_root).await;
    assert_eq!(second_ack.bridge_instance_id, first_ack.bridge_instance_id);
    assert_ne!(second_ack.secret_ref, first_ack.secret_ref);

    let capability = capability_snapshot("bridge-session");
    let capability_bytes = serde_json::to_vec(&capability).expect("json");
    let stale_headers = sign_headers(
        "POST",
        "/v1/ingress/capability-snapshot",
        &capability_bytes,
        &first_ack.bridge_instance_id,
        &first_ack.secret_ref,
        first_ack.rotation_epoch,
        &first_ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/capability-snapshot")
        .header("content-type", "application/json");
    for (name, value) in &stale_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(capability_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authenticated_ingress_and_outbound_require_bootstrap_headers() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/ingress/current-turn")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&current_turn("bridge-session")).expect("json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let ack = bootstrap(app.clone(), &runtime_root).await;

    let turn_bytes = serde_json::to_vec(&current_turn("bridge-session")).expect("json");
    let turn_headers = sign_headers(
        "POST",
        "/v1/ingress/current-turn",
        &turn_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/current-turn")
        .header("content-type", "application/json");
    for (name, value) in &turn_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(turn_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let capability = capability_snapshot("bridge-session");
    let capability_bytes = serde_json::to_vec(&capability).expect("json");
    let capability_headers = sign_headers(
        "POST",
        "/v1/ingress/capability-snapshot",
        &capability_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/capability-snapshot")
        .header("content-type", "application/json");
    for (name, value) in &capability_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(capability_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let semantic_bytes = serde_json::to_vec(&managed_candidate("bridge-session")).expect("json");
    let semantic_headers = sign_headers(
        "POST",
        "/v1/ingress/semantic-decision",
        &semantic_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/semantic-decision")
        .header("content-type", "application/json");
    for (name, value) in &semantic_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(semantic_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let outbound_headers = sign_headers(
        "GET",
        "/v1/outbound/next?host_session_id=bridge-session",
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("/v1/outbound/next?host_session_id=bridge-session");
    for (name, value) in &outbound_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn semantic_decision_ingress_accepts_task_change_governance_fields_for_active_task() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let session = "bridge-session".to_string();
    harness
        .ingest_current_turn(current_turn(&session))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot(&session))
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(managed_candidate(&session))
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&session)
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
        .expect("approve start");

    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;
    let payload = SemanticDecisionEnvelope {
        decision_id: new_id("decision"),
        host_session_id: session.clone(),
        host_message_ref: Some("host-message-2".into()),
        managed_task_ref: Some(task.managed_task_ref),
        interaction_lane: loom_domain::InteractionLane::ManagedTaskActive,
        managed_task_class: Some(loom_domain::ManagedTaskClass::Complex),
        work_horizon: Some(loom_domain::WorkHorizonKind::Improvement),
        task_activation_reason: Some(loom_domain::TaskActivationReason::ExplicitStartTask),
        task_change_classification: Some(loom_domain::TaskChangeClassification::SameTaskMinor),
        task_change_execution_surface: Some(loom_domain::ChangeExecutionSurface::FutureOnly),
        task_change_boundary_recommendation: Some(
            loom_domain::BoundaryRecommendation::AbsorbChange,
        ),
        title: None,
        summary: Some("Update the active task.".into()),
        expected_outcome: None,
        requirement_items: Vec::new(),
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
        allowed_roots: vec!["/Users/codez/.openclaw".into()],
        secret_classes: vec!["repo".into()],
        confidence: Some(92),
        created_at: now_timestamp(),
    };
    let body = serde_json::to_vec(&payload).expect("json");
    let path = "/v1/ingress/semantic-decision";
    let headers = sign_headers(
        "POST",
        path,
        &body,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn semantic_bundle_ingress_accepts_paired_request_task_change_and_persists_judgment_fact() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let session = "bridge-batch-session".to_string();
    harness
        .ingest_current_turn(current_turn(&session))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot(&session))
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(managed_candidate(&session))
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&session)
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
        .expect("approve start");
    let stage_notice = harness
        .store()
        .next_outbound(&session)
        .expect("stage notice outbound")
        .expect("stage notice");
    harness
        .store()
        .ack_outbound(&stage_notice.delivery_id)
        .expect("ack stage notice");

    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;
    let source_decision_ref = "decision-task-change";
    let batch_issued_at = now_timestamp();
    let interaction_issued_at = now_timestamp();
    let task_change_issued_at = now_timestamp();
    let control_action_issued_at = now_timestamp();
    let body = serde_json::to_vec(&serde_json::json!({
        "meta": {
            "ingress_id": "ingress-batch-1",
            "received_at": now_timestamp(),
            "causation_id": null,
            "correlation_id": "corr-batch-1",
            "dedupe_window": "PT10M"
        },
        "host_session_id": session,
        "host_message_ref": "host-message-2",
        "input_ref": "host-message-2",
        "source_model_ref": "host-model",
        "issued_at": batch_issued_at,
        "rationale_summary": "paired request_task_change batch",
        "semantic_decisions": [
            {
                "decision_ref": "decision-interaction",
                "host_session_id": session,
                "host_message_ref": "host-message-2",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "interaction_lane",
                "decision_source": "host_model",
                "rationale": "the turn targets the active task",
                "confidence": 95,
                "source_model_ref": "host-model",
                "issued_at": interaction_issued_at,
                "decision_payload": {
                    "interaction_lane": "managed_task_active",
                    "managed_task_ref": task.managed_task_ref,
                    "title": null,
                    "summary": "Update the active task",
                    "expected_outcome": null,
                    "requirement_items": [],
                    "workspace_ref": null,
                    "repo_ref": null,
                    "allowed_roots": [],
                    "secret_classes": []
                }
            },
            {
                "decision_ref": source_decision_ref,
                "host_session_id": session,
                "host_message_ref": "host-message-2",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "task_change",
                "decision_source": "host_model",
                "rationale": "future-only same-task change",
                "confidence": 88,
                "source_model_ref": "host-model",
                "issued_at": task_change_issued_at,
                "decision_payload": {
                    "classification": "same_task_minor",
                    "execution_surface": "future_only",
                    "boundary_recommendation": "absorb_change"
                }
            }
        ],
        "control_action": {
            "decision_ref": "decision-control",
            "decision_source": "user_control_action",
            "rationale": "explicit request to change the active task",
            "confidence": 99,
            "source_model_ref": "host-model",
            "issued_at": control_action_issued_at,
            "action": {
                "action_id": "action-batch-1",
                "managed_task_ref": task.managed_task_ref,
                "kind": "request_task_change",
                "actor": "user",
                "payload": {
                    "title": null,
                    "summary": "Expand the task into the notification workspace.",
                    "expected_outcome": "Task context and scope both point at notifier.",
                    "requirement_items": [],
                    "allowed_roots": ["/Users/codez/.openclaw", "/Users/codez/.openclaw/notification"],
                    "secret_classes": ["repo", "dev"],
                    "workspace_ref": "/Users/codez/.openclaw/notification",
                    "repo_ref": "openclaw-notifier",
                    "rationale": "user expanded the same task into a sibling workspace"
                },
                "source_decision_ref": source_decision_ref,
                "decision_token": null
            }
        }
    }))
    .expect("json");
    let path = "/v1/ingress/semantic-bundle";
    let headers = sign_headers(
        "POST",
        path,
        &body,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let stored_decision = harness
        .store()
        .load_semantic_decision(source_decision_ref)
        .expect("stored semantic decision")
        .expect("decision exists");
    assert_eq!(stored_decision.decision_ref, source_decision_ref);
    let latest_scope = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("scope")
        .expect("latest scope");
    assert_eq!(latest_scope.scope_version, 2);
    assert_eq!(
        latest_scope.workspace_ref.as_deref(),
        Some("/Users/codez/.openclaw/notification")
    );

    let duplicate_body = serde_json::to_vec(&serde_json::json!({
        "meta": {
            "ingress_id": "ingress-batch-2",
            "received_at": now_timestamp(),
            "causation_id": null,
            "correlation_id": "corr-batch-2",
            "dedupe_window": "PT10M"
        },
        "host_session_id": session,
        "host_message_ref": "host-message-2",
        "input_ref": "host-message-2",
        "source_model_ref": "host-model",
        "issued_at": batch_issued_at,
        "rationale_summary": "paired request_task_change batch duplicate",
        "semantic_decisions": [
            {
                "decision_ref": "decision-interaction",
                "host_session_id": session,
                "host_message_ref": "host-message-2",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "interaction_lane",
                "decision_source": "host_model",
                "rationale": "the turn targets the active task",
                "confidence": 95,
                "source_model_ref": "host-model",
                "issued_at": interaction_issued_at,
                "decision_payload": {
                    "interaction_lane": "managed_task_active",
                    "managed_task_ref": task.managed_task_ref,
                    "title": null,
                    "summary": "Update the active task",
                    "expected_outcome": null,
                    "requirement_items": [],
                    "workspace_ref": null,
                    "repo_ref": null,
                    "allowed_roots": [],
                    "secret_classes": []
                }
            },
            {
                "decision_ref": source_decision_ref,
                "host_session_id": session,
                "host_message_ref": "host-message-2",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "task_change",
                "decision_source": "host_model",
                "rationale": "future-only same-task change",
                "confidence": 88,
                "source_model_ref": "host-model",
                "issued_at": task_change_issued_at,
                "decision_payload": {
                    "classification": "same_task_minor",
                    "execution_surface": "future_only",
                    "boundary_recommendation": "absorb_change"
                }
            }
        ],
        "control_action": {
            "decision_ref": "decision-control",
            "decision_source": "user_control_action",
            "rationale": "explicit request to change the active task",
            "confidence": 99,
            "source_model_ref": "host-model",
            "issued_at": control_action_issued_at,
            "action": {
                "action_id": "action-batch-1",
                "managed_task_ref": task.managed_task_ref,
                "kind": "request_task_change",
                "actor": "user",
                "payload": {
                    "title": null,
                    "summary": "Expand the task into the notification workspace.",
                    "expected_outcome": "Task context and scope both point at notifier.",
                    "requirement_items": [],
                    "allowed_roots": ["/Users/codez/.openclaw", "/Users/codez/.openclaw/notification"],
                    "secret_classes": ["repo", "dev"],
                    "workspace_ref": "/Users/codez/.openclaw/notification",
                    "repo_ref": "openclaw-notifier",
                    "rationale": "user expanded the same task into a sibling workspace"
                },
                "source_decision_ref": source_decision_ref,
                "decision_token": null
            }
        }
    }))
    .expect("duplicate json");
    let duplicate_headers = sign_headers(
        "POST",
        path,
        &duplicate_body,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut duplicate_builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in &duplicate_headers {
        duplicate_builder = duplicate_builder.header(*name, value);
    }
    let duplicate_response = app
        .clone()
        .oneshot(
            duplicate_builder
                .body(Body::from(duplicate_body))
                .expect("duplicate request"),
        )
        .await
        .expect("duplicate response");
    assert_eq!(duplicate_response.status(), StatusCode::ACCEPTED);
    let latest_scope_after_duplicate = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("scope after duplicate")
        .expect("latest scope after duplicate");
    assert_eq!(latest_scope_after_duplicate.scope_version, 2);

    let conflicting_body = serde_json::to_vec(&serde_json::json!({
        "meta": {
            "ingress_id": "ingress-batch-3",
            "received_at": now_timestamp(),
            "causation_id": null,
            "correlation_id": "corr-batch-3",
            "dedupe_window": "PT10M"
        },
        "host_session_id": session,
        "host_message_ref": "host-message-2",
        "input_ref": "host-message-2",
        "source_model_ref": "host-model",
        "issued_at": batch_issued_at,
        "rationale_summary": "paired request_task_change batch conflicting replay",
        "semantic_decisions": [
            {
                "decision_ref": "decision-interaction",
                "host_session_id": session,
                "host_message_ref": "host-message-2",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "interaction_lane",
                "decision_source": "host_model",
                "rationale": "the turn targets the active task",
                "confidence": 95,
                "source_model_ref": "host-model",
                "issued_at": interaction_issued_at,
                "decision_payload": {
                    "interaction_lane": "managed_task_active",
                    "managed_task_ref": task.managed_task_ref,
                    "title": null,
                    "summary": "Update the active task",
                    "expected_outcome": null,
                    "requirement_items": [],
                    "workspace_ref": null,
                    "repo_ref": null,
                    "allowed_roots": [],
                    "secret_classes": []
                }
            },
            {
                "decision_ref": source_decision_ref,
                "host_session_id": session,
                "host_message_ref": "host-message-2",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "task_change",
                "decision_source": "host_model",
                "rationale": "conflicting replay changes the meaning of the same judgment",
                "confidence": 88,
                "source_model_ref": "host-model",
                "issued_at": task_change_issued_at,
                "decision_payload": {
                    "classification": "same_task_minor",
                    "execution_surface": "future_only",
                    "boundary_recommendation": "absorb_change"
                }
            }
        ],
        "control_action": {
            "decision_ref": "decision-control",
            "decision_source": "user_control_action",
            "rationale": "explicit request to change the active task",
            "confidence": 99,
            "source_model_ref": "host-model",
            "issued_at": control_action_issued_at,
            "action": {
                "action_id": "action-batch-1",
                "managed_task_ref": task.managed_task_ref,
                "kind": "request_task_change",
                "actor": "user",
                "payload": {
                    "title": null,
                    "summary": "Expand the task into the notification workspace.",
                    "expected_outcome": "Task context and scope both point at notifier.",
                    "requirement_items": [],
                    "allowed_roots": ["/Users/codez/.openclaw", "/Users/codez/.openclaw/notification"],
                    "secret_classes": ["repo", "dev"],
                    "workspace_ref": "/Users/codez/.openclaw/notification",
                    "repo_ref": "openclaw-notifier",
                    "rationale": "user expanded the same task into a sibling workspace"
                },
                "source_decision_ref": source_decision_ref,
                "decision_token": null
            }
        }
    }))
    .expect("conflicting json");
    let conflicting_headers = sign_headers(
        "POST",
        path,
        &conflicting_body,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut conflicting_builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in &conflicting_headers {
        conflicting_builder = conflicting_builder.header(*name, value);
    }
    let conflicting_response = app
        .clone()
        .oneshot(
            conflicting_builder
                .body(Body::from(conflicting_body))
                .expect("conflicting request"),
        )
        .await
        .expect("conflicting response");
    assert_eq!(conflicting_response.status(), StatusCode::BAD_REQUEST);
    let conflict_status = harness
        .store()
        .semantic_decision_batch_status("ingress-batch-3")
        .expect("conflict batch status")
        .expect("conflict batch exists");
    assert_eq!(conflict_status, "rejected");
    let latest_scope_after_conflict = harness
        .store()
        .latest_scope_snapshot(&task.managed_task_ref)
        .expect("scope after conflict")
        .expect("latest scope after conflict");
    assert_eq!(latest_scope_after_conflict.scope_version, 2);
}

#[tokio::test]
async fn semantic_bundle_ingress_rejects_unpaired_request_task_change_and_keeps_only_rejected_batch() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let session = "bridge-batch-reject".to_string();
    harness
        .ingest_current_turn(current_turn(&session))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot(&session))
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(managed_candidate(&session))
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&session)
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
        .expect("approve start");
    let stage_notice = harness
        .store()
        .next_outbound(&session)
        .expect("stage notice outbound")
        .expect("stage notice");
    harness
        .store()
        .ack_outbound(&stage_notice.delivery_id)
        .expect("ack stage notice");

    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;
    let batch_ref = "ingress-batch-reject-1";
    let body = serde_json::to_vec(&serde_json::json!({
        "meta": {
            "ingress_id": batch_ref,
            "received_at": now_timestamp(),
            "causation_id": null,
            "correlation_id": "corr-batch-reject-1",
            "dedupe_window": "PT10M"
        },
        "host_session_id": session,
        "host_message_ref": "host-message-3",
        "input_ref": "host-message-3",
        "source_model_ref": "host-model",
        "issued_at": now_timestamp(),
        "rationale_summary": "unpaired request_task_change batch",
        "semantic_decisions": [
            {
                "decision_ref": "decision-interaction-reject",
                "host_session_id": session,
                "host_message_ref": "host-message-3",
                "managed_task_ref": task.managed_task_ref,
                "decision_kind": "interaction_lane",
                "decision_source": "host_model",
                "rationale": "the turn targets the active task",
                "confidence": 95,
                "source_model_ref": "host-model",
                "issued_at": now_timestamp(),
                "decision_payload": {
                    "interaction_lane": "managed_task_active",
                    "managed_task_ref": task.managed_task_ref,
                    "title": null,
                    "summary": "Update the active task",
                    "expected_outcome": null,
                    "requirement_items": [],
                    "workspace_ref": null,
                    "repo_ref": null,
                    "allowed_roots": [],
                    "secret_classes": []
                }
            }
        ],
        "control_action": {
            "decision_ref": "decision-control-reject",
            "decision_source": "user_control_action",
            "rationale": "explicit request to change the active task",
            "confidence": 99,
            "source_model_ref": "host-model",
            "issued_at": now_timestamp(),
            "action": {
                "action_id": "action-batch-reject-1",
                "managed_task_ref": task.managed_task_ref,
                "kind": "request_task_change",
                "actor": "user",
                "payload": {
                    "title": null,
                    "summary": "Expand the task into the notification workspace.",
                    "expected_outcome": "Task context and scope both point at notifier.",
                    "requirement_items": [],
                    "allowed_roots": [],
                    "secret_classes": [],
                    "workspace_ref": "/Users/codez/.openclaw/notification",
                    "repo_ref": "openclaw-notifier",
                    "rationale": null
                },
                "source_decision_ref": "decision-missing",
                "decision_token": null
            }
        }
    }))
    .expect("json");
    let path = "/v1/ingress/semantic-bundle";
    let headers = sign_headers(
        "POST",
        path,
        &body,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .oneshot(builder.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    assert_eq!(
        harness
            .store()
            .semantic_decision_batch_status(batch_ref)
            .expect("batch status")
            .as_deref(),
        Some("rejected")
    );
    assert!(
        harness
            .store()
            .load_semantic_decision("decision-missing")
            .expect("missing semantic decision")
            .is_none()
    );
    let events = harness
        .store()
        .list_task_events(&task.managed_task_ref)
        .expect("task events");
    assert!(events.iter().any(|event| {
        event.event_name == "task_change_clarification_requested"
    }));
    let blocked_notice = harness
        .store()
        .next_outbound(&session)
        .expect("clarification notice outbound")
        .expect("clarification notice");
    let loom_domain::KernelOutboundPayload::StatusNotice(blocked_notice) = blocked_notice.payload
    else {
        panic!("expected blocked status notice");
    };
    assert_eq!(blocked_notice.headline, "Task change needs clarification");
}

#[tokio::test]
async fn outbound_query_decodes_percent_encoded_host_session_id() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;
    let session_id = "agent:main:main";

    let turn_bytes = serde_json::to_vec(&current_turn(session_id)).expect("json");
    let turn_headers = sign_headers(
        "POST",
        "/v1/ingress/current-turn",
        &turn_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/current-turn")
        .header("content-type", "application/json");
    for (name, value) in &turn_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(turn_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let capability_bytes = serde_json::to_vec(&capability_snapshot(session_id)).expect("json");
    let capability_headers = sign_headers(
        "POST",
        "/v1/ingress/capability-snapshot",
        &capability_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/capability-snapshot")
        .header("content-type", "application/json");
    for (name, value) in &capability_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(capability_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let semantic_bytes = serde_json::to_vec(&managed_candidate(session_id)).expect("json");
    let semantic_path = "/v1/ingress/semantic-decision";
    let semantic_headers = sign_headers(
        "POST",
        semantic_path,
        &semantic_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(semantic_path)
        .header("content-type", "application/json");
    for (name, value) in &semantic_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(semantic_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let next_path = "/v1/outbound/next?host_session_id=agent%3Amain%3Amain";
    let next_headers = sign_headers(
        "GET",
        next_path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::GET).uri(next_path);
    for (name, value) in &next_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn outbound_retry_route_schedules_backoff_before_redelivery() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    harness
        .ingest_capability_snapshot(capability_snapshot("bridge-session"))
        .expect("capability snapshot");
    harness
        .ingest_semantic_decision(managed_candidate("bridge-session"))
        .expect("candidate")
        .expect("managed task");

    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;

    let next_path = "/v1/outbound/next?host_session_id=bridge-session";
    let next_headers = sign_headers(
        "GET",
        next_path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::GET).uri(next_path);
    for (name, value) in &next_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let outbound: OutboundDelivery = serde_json::from_slice(&bytes).expect("outbound");

    let future_attempt_at = (now_timestamp().parse::<u128>().expect("millis") + 60_000).to_string();
    let retry_path = format!("/v1/outbound/{}/retry", outbound.delivery_id);
    let retry_body = serde_json::to_vec(&serde_json::json!({
        "next_attempt_at": future_attempt_at,
        "last_error": "transcript file not found"
    }))
    .expect("retry body");
    let retry_headers = sign_headers(
        "POST",
        &retry_path,
        &retry_body,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(&retry_path)
        .header("content-type", "application/json");
    for (name, value) in &retry_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(retry_body)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let stored = harness
        .store()
        .load_outbound(&outbound.delivery_id)
        .expect("load outbound")
        .expect("outbound exists");
    assert_eq!(stored.delivery_status, DeliveryStatus::RetryScheduled);
    assert_eq!(
        stored.next_attempt_at.as_deref(),
        Some(future_attempt_at.as_str())
    );
    assert_eq!(
        stored.last_error.as_deref(),
        Some("transcript file not found")
    );

    let next_headers = sign_headers(
        "GET",
        next_path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::GET).uri(next_path);
    for (name, value) in &next_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn unauthorized_ingress_is_audited_with_auth_failure_reason() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/ingress/current-turn")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&current_turn("bridge-session")).expect("json"),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let audits = read_bridge_audits(&runtime_root);
    let latest = audits.last().expect("latest audit");
    assert_eq!(latest["event_name"], "bridge.auth.failed");
    assert_eq!(latest["payload"]["path"], "/v1/ingress/current-turn");
    assert_eq!(
        latest["payload"]["reason"],
        "missing auth header: x-loom-bridge-instance-id"
    );
}

#[tokio::test]
async fn control_action_missing_decision_token_returns_bad_request_after_auth() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;

    let capability = capability_snapshot("bridge-session");
    let response = app
        .clone()
        .oneshot({
            let bytes = serde_json::to_vec(&capability).expect("json");
            let headers = sign_headers(
                "POST",
                "/v1/ingress/capability-snapshot",
                &bytes,
                &ack.bridge_instance_id,
                &ack.secret_ref,
                ack.rotation_epoch,
                &ack.session_secret,
            );
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/v1/ingress/capability-snapshot")
                .header("content-type", "application/json");
            for (name, value) in &headers {
                builder = builder.header(*name, value);
            }
            builder.body(Body::from(bytes)).expect("request")
        })
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = app
        .clone()
        .oneshot({
            let bytes = serde_json::to_vec(&managed_candidate("bridge-session")).expect("json");
            let headers = sign_headers(
                "POST",
                "/v1/ingress/semantic-decision",
                &bytes,
                &ack.bridge_instance_id,
                &ack.secret_ref,
                ack.rotation_epoch,
                &ack.session_secret,
            );
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/v1/ingress/semantic-decision")
                .header("content-type", "application/json");
            for (name, value) in &headers {
                builder = builder.header(*name, value);
            }
            builder.body(Body::from(bytes)).expect("request")
        })
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let control = ControlAction {
        action_id: new_id("action"),
        managed_task_ref: Some(new_id("task")),
        kind: ControlActionKind::ApproveStart,
        actor: loom_domain::ControlActorRef::User,
        payload: ControlActionPayload::default(),
        source_decision_ref: None,
        decision_token: None,
    };
    let bytes = serde_json::to_vec(&control).expect("json");
    let headers = sign_headers(
        "POST",
        "/v1/ingress/control-action",
        &bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/control-action")
        .header("content-type", "application/json");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .oneshot(builder.body(Body::from(bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn legacy_request_task_change_without_source_judgment_returns_bad_request_after_auth() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;
    let session = "bridge-legacy-task-change";

    harness
        .ingest_current_turn(current_turn(session))
        .expect("current turn");
    harness
        .ingest_capability_snapshot(capability_snapshot(session))
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(managed_candidate(session))
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
            source_decision_ref: Some(new_id("decision-ref")),
            decision_token: Some(start_card.decision_token),
        })
        .expect("approve start");
    let stage_notice = harness
        .store()
        .next_outbound(&session.to_string())
        .expect("stage notice outbound")
        .expect("stage notice");
    harness
        .store()
        .ack_outbound(&stage_notice.delivery_id)
        .expect("ack stage notice");

    let control = ControlAction {
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
    };
    let bytes = serde_json::to_vec(&control).expect("json");
    let headers = sign_headers(
        "POST",
        "/v1/ingress/control-action",
        &bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/v1/ingress/control-action")
        .header("content-type", "application/json");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .oneshot(builder.body(Body::from(bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let blocked_notice = harness
        .store()
        .next_outbound(&session.to_string())
        .expect("clarification notice outbound")
        .expect("clarification notice");
    let loom_domain::KernelOutboundPayload::StatusNotice(blocked_notice) = blocked_notice.payload
    else {
        panic!("expected blocked status notice");
    };
    assert_eq!(blocked_notice.headline, "Task change needs clarification");
}

#[tokio::test]
async fn current_control_surface_query_reads_authoritative_open_window_for_session() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;

    harness
        .ingest_semantic_decision(managed_candidate("bridge-session"))
        .expect("candidate")
        .expect("managed task");

    let path = "/v1/control-surface/current?host_session_id=bridge-session";
    let headers = sign_headers(
        "GET",
        path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::GET).uri(path);
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let projection: Value = serde_json::from_slice(&bytes).expect("projection");
    assert_eq!(projection["surface_type"], "start_card");
    assert_eq!(projection["host_session_id"], "bridge-session");
    assert_eq!(projection["allowed_actions"][0], "approve_start");
    assert_eq!(projection["allowed_actions"][1], "modify_candidate");
    assert_eq!(projection["allowed_actions"][2], "cancel_candidate");
}

#[tokio::test]
async fn current_control_surface_query_fails_closed_when_session_has_multiple_open_windows() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;

    harness
        .ingest_semantic_decision(managed_candidate("bridge-session"))
        .expect("candidate one")
        .expect("managed task one");
    harness
        .ingest_semantic_decision(managed_candidate("bridge-session"))
        .expect("candidate two")
        .expect("managed task two");

    let path = "/v1/control-surface/current?host_session_id=bridge-session";
    let headers = sign_headers(
        "GET",
        path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::GET).uri(path);
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn host_execution_routes_require_auth_and_accept_lifecycle_ingress() {
    let harness = harness();
    let runtime_root = harness.store().runtime_root().to_path_buf();
    harness
        .ingest_capability_snapshot(capability_snapshot("bridge-session"))
        .expect("capability snapshot");
    let task = harness
        .ingest_semantic_decision(managed_candidate("bridge-session"))
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&"bridge-session".to_string())
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
        .expect("approve start");

    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;

    let next_path = "/v1/host-execution/next?host_session_id=bridge-session";
    let next_headers = sign_headers(
        "GET",
        next_path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::GET).uri(next_path);
    for (name, value) in &next_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let command: loom_domain::HostExecutionCommand =
        serde_json::from_slice(&bytes).expect("command");

    let ack_path = format!("/v1/host-execution/{}/ack", command.command_id);
    let ack_headers = sign_headers(
        "POST",
        &ack_path,
        &[],
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder().method(Method::POST).uri(&ack_path);
    for (name, value) in &ack_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let lifecycle = loom_domain::HostSubagentLifecycleEnvelope {
        meta: IngressMeta::default(),
        command_id: command.command_id.clone(),
        managed_task_ref: task.managed_task_ref.clone(),
        run_ref: command.run_ref.clone(),
        role_kind: loom_domain::AgentRoleKind::Worker,
        event: loom_domain::HostSubagentLifecycleEvent::Spawned(
            loom_domain::SubagentSpawnedPayload {
                host_child_execution_ref: "agent:coder:child-1".into(),
                host_child_run_ref: Some("run-child-1".into()),
                host_agent_id: "coder".into(),
                observed_at: now_timestamp(),
            },
        ),
    };
    let lifecycle_bytes = serde_json::to_vec(&lifecycle).expect("json");
    let lifecycle_path = "/v1/ingress/subagent-lifecycle";
    let lifecycle_headers = sign_headers(
        "POST",
        lifecycle_path,
        &lifecycle_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(lifecycle_path)
        .header("content-type", "application/json");
    for (name, value) in &lifecycle_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(lifecycle_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let command_after = harness
        .store()
        .load_host_execution_command(&command.command_id)
        .expect("command after")
        .expect("command exists");
    assert_eq!(
        command_after.status,
        loom_domain::HostExecutionCommandStatus::Running
    );
}

#[tokio::test]
async fn host_execution_lifecycle_accepts_legacy_child_aliases_on_ingress() {
    let runtime_root = tempdir().expect("tempdir").keep();
    let store = LoomStore::in_memory(runtime_root.clone()).expect("store");
    let harness = LoomHarness::new(store);

    let session = "bridge-session".to_string();
    harness
        .ingest_current_turn(CurrentTurnEnvelope {
            meta: IngressMeta::default(),
            host_session_id: session.clone(),
            host_message_ref: Some("host-message-1".into()),
            text: "Please start the Loom bridge task".into(),
            workspace_ref: Some("/Users/codez/.openclaw".into()),
            repo_ref: Some("openclaw".into()),
        })
        .expect("current turn");
    harness
        .ingest_capability_snapshot(HostCapabilitySnapshot {
            capability_snapshot_ref: new_id("cap"),
            host_session_id: session.clone(),
            allowed_tools: vec!["read_file".into(), "write_file".into()],
            readable_roots: vec!["/Users/codez/.openclaw".into()],
            writable_roots: vec!["/Users/codez/.openclaw".into()],
            secret_classes: vec!["repo".into()],
            max_budget_band: loom_domain::AuthorizationBudgetBand::Standard,
            available_agent_ids: vec!["coder".into(), "product_analyst".into()],
            supports_spawn_agents: true,
            supports_pause: true,
            supports_resume: true,
            supports_interrupt: true,
            recorded_at: now_timestamp(),
        })
        .expect("capability");
    let task = harness
        .ingest_semantic_decision(SemanticDecisionEnvelope {
            decision_id: new_id("decision"),
            host_session_id: session.clone(),
            host_message_ref: Some("host-message-1".into()),
            managed_task_ref: None,
            interaction_lane: loom_domain::InteractionLane::ManagedTaskCandidate,
            managed_task_class: Some(loom_domain::ManagedTaskClass::Complex),
            work_horizon: Some(loom_domain::WorkHorizonKind::Improvement),
            task_activation_reason: Some(loom_domain::TaskActivationReason::ExplicitStartTask),
            task_change_classification: None,
            task_change_execution_surface: None,
            task_change_boundary_recommendation: None,
            title: Some("Bridge task".into()),
            summary: Some("Create a managed task candidate.".into()),
            expected_outcome: Some("Task enters execute after approval.".into()),
            requirement_items: vec![loom_domain::RequirementItemDraft {
                text: "Start from current turn".into(),
                origin: loom_domain::RequirementOrigin::InitialDecision,
            }],
            workspace_ref: Some("/Users/codez/.openclaw".into()),
            repo_ref: Some("openclaw".into()),
            allowed_roots: vec!["/Users/codez/.openclaw".into()],
            secret_classes: vec!["repo".into()],
            confidence: Some(97),
            created_at: now_timestamp(),
        })
        .expect("candidate")
        .expect("managed task");
    let outbound = harness
        .store()
        .next_outbound(&session)
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
        .expect("approve start");

    let app = build_router(harness.clone());
    let ack = bootstrap(app.clone(), &runtime_root).await;
    let command = harness
        .store()
        .list_host_execution_commands(&task.managed_task_ref)
        .expect("commands")
        .into_iter()
        .find(|value| value.role_kind == loom_domain::AgentRoleKind::Worker)
        .expect("worker command");

    let legacy_lifecycle = serde_json::json!({
        "meta": {
            "ingress_id": "ingress-legacy",
            "received_at": now_timestamp(),
            "causation_id": command.command_id,
            "correlation_id": "corr-legacy",
            "dedupe_window": "PT10M"
        },
        "command_id": command.command_id,
        "managed_task_ref": task.managed_task_ref,
        "run_ref": command.run_ref,
        "role_kind": "worker",
        "event": {
            "spawned": {
                "child_session_key": "agent:coder:child-legacy",
                "child_run_id": "run-child-legacy",
                "host_agent_id": "coder",
                "observed_at": now_timestamp()
            }
        }
    });
    let lifecycle_bytes = serde_json::to_vec(&legacy_lifecycle).expect("json");
    let lifecycle_path = "/v1/ingress/subagent-lifecycle";
    let lifecycle_headers = sign_headers(
        "POST",
        lifecycle_path,
        &lifecycle_bytes,
        &ack.bridge_instance_id,
        &ack.secret_ref,
        ack.rotation_epoch,
        &ack.session_secret,
    );
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(lifecycle_path)
        .header("content-type", "application/json");
    for (name, value) in &lifecycle_headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(lifecycle_bytes)).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let command_after = harness
        .store()
        .load_host_execution_command(&command.command_id)
        .expect("command after")
        .expect("command exists");
    assert_eq!(
        command_after.host_child_execution_ref.as_deref(),
        Some("agent:coder:child-legacy")
    );
}

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use loom_bridge_http::{BOOTSTRAP_TICKET_RELATIVE_PATH, build_router};
use loom_domain::{
    AuthorizationBudgetBand, BridgeBootstrapAck, BridgeBootstrapMaterial, BridgeHealthResponse,
    ControlAction, ControlActionKind, ControlActionPayload, CurrentTurnEnvelope,
    HostCapabilitySnapshot, IngressMeta, InteractionLane, ManagedTaskClass, RequirementItemDraft,
    RequirementOrigin, SemanticDecisionEnvelope, TaskActivationReason, WorkHorizonKind, new_id,
    now_timestamp,
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
        task_activation_reason: Some(TaskActivationReason::ExplicitUserRequest),
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

    assert_eq!(rotated_material.bridge_instance_id, first_material.bridge_instance_id);
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
        .oneshot(
            {
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
            },
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = app
        .clone()
        .oneshot(
            {
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
            },
        )
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
        .oneshot(
            builder.body(Body::from(bytes)).expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
        .oneshot(
            builder
                .body(Body::from(lifecycle_bytes))
                .expect("request"),
        )
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
            task_activation_reason: Some(loom_domain::TaskActivationReason::ExplicitUserRequest),
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
        .oneshot(
            builder
                .body(Body::from(lifecycle_bytes))
                .expect("request"),
        )
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

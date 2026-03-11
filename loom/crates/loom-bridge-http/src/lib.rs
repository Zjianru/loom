use axum::body::{Body, to_bytes};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use loom_domain::{
    BridgeAuthEnvelope, BridgeBootstrapAck, BridgeBootstrapMaterial, BridgeBootstrapRequest,
    BridgeBootstrapTicket, BridgeCredentialStatus, BridgeHealthResponse, BridgeSessionCredential,
    ControlAction, CurrentTurnEnvelope, HostCapabilitySnapshot, HostExecutionCommand,
    HostSessionId, HostSubagentLifecycleEnvelope, OutboundDelivery, SemanticDecisionEnvelope,
    new_id, now_timestamp,
};
use loom_harness::LoomHarness;
use serde::de::DeserializeOwned;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

pub const DEFAULT_ADAPTER_ID: &str = "loom-openclaw";
pub const BOOTSTRAP_TICKET_RELATIVE_PATH: &str = "bootstrap/openclaw/bootstrap-ticket.json";

#[derive(Clone)]
pub struct BridgeState {
    pub harness: Arc<LoomHarness>,
    pub bridge_instance_id: Arc<String>,
    pub session_secrets: Arc<Mutex<HashMap<String, String>>>,
}

pub fn build_router(harness: LoomHarness) -> Router {
    let state = init_bridge_state(harness).expect("initializing LocalHttpBridge state");
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/bootstrap", post(bootstrap))
        .route("/v1/ingress/current-turn", post(ingest_current_turn))
        .route(
            "/v1/ingress/capability-snapshot",
            post(ingest_capability_snapshot),
        )
        .route(
            "/v1/ingress/semantic-decision",
            post(ingest_semantic_decision),
        )
        .route("/v1/ingress/control-action", post(ingest_control_action))
        .route(
            "/v1/ingress/subagent-lifecycle",
            post(ingest_subagent_lifecycle),
        )
        .route("/v1/outbound/next", get(get_next_outbound))
        .route("/v1/outbound/{delivery_id}/ack", post(ack_outbound))
        .route("/v1/host-execution/next", get(get_next_host_execution))
        .route(
            "/v1/host-execution/{command_id}/ack",
            post(ack_host_execution),
        )
        .with_state(state)
}

fn init_bridge_state(harness: LoomHarness) -> anyhow::Result<BridgeState> {
    let bridge_instance_id = new_id("bridge");
    issue_bootstrap_ticket(&harness, &bridge_instance_id, DEFAULT_ADAPTER_ID)?;
    log_bridge_event(
        "bridge.runtime.started",
        json!({
            "bridge_instance_id": bridge_instance_id,
        }),
    );
    Ok(BridgeState {
        harness: Arc::new(harness),
        bridge_instance_id: Arc::new(bridge_instance_id),
        session_secrets: Arc::new(Mutex::new(HashMap::new())),
    })
}

fn log_bridge_event(event_name: &str, payload: serde_json::Value) {
    println!(
        "{}",
        serde_json::json!({
            "event": event_name,
            "payload": payload,
        })
    );
}

fn future_timestamp_ms(delta_ms: u128) -> String {
    let now = now_timestamp().parse::<u128>().unwrap_or_default();
    (now + delta_ms).to_string()
}

async fn health(State(state): State<BridgeState>) -> Json<BridgeHealthResponse> {
    Json(BridgeHealthResponse {
        bridge_instance_id: (*state.bridge_instance_id).clone(),
        status: "ready".into(),
    })
}

fn issue_bootstrap_ticket(
    harness: &LoomHarness,
    bridge_instance_id: &str,
    adapter_id: &str,
) -> anyhow::Result<BridgeBootstrapMaterial> {
    let ticket_secret = new_id("ticket-secret");
    let ticket = BridgeBootstrapTicket {
        ticket_id: new_id("ticket"),
        bridge_instance_id: bridge_instance_id.into(),
        adapter_id: adapter_id.into(),
        issued_at: now_timestamp(),
        expires_at: future_timestamp_ms(10 * 60 * 1000),
        ticket_secret_hash: harness.store().hash_bridge_secret(&ticket_secret),
        status: BridgeCredentialStatus::PendingBootstrap,
    };
    let material = BridgeBootstrapMaterial {
        bridge_instance_id: bridge_instance_id.into(),
        adapter_id: adapter_id.into(),
        ticket_id: ticket.ticket_id.clone(),
        ticket_secret,
        issued_at: ticket.issued_at.clone(),
        expires_at: ticket.expires_at.clone(),
    };
    harness.store().save_bridge_bootstrap_ticket(&ticket, &material)?;
    harness.store().append_bridge_auth_audit(
        "bridge.bootstrap.ticket_issued",
        json!({
            "bridge_instance_id": bridge_instance_id,
            "ticket_id": ticket.ticket_id,
        }),
    )?;
    log_bridge_event(
        "bridge.bootstrap.ticket_issued",
        json!({
            "bridge_instance_id": bridge_instance_id,
            "ticket_id": ticket.ticket_id,
        }),
    );
    Ok(material)
}

async fn bootstrap(
    State(state): State<BridgeState>,
    Json(payload): Json<BridgeBootstrapRequest>,
) -> Result<Json<BridgeBootstrapAck>, (StatusCode, String)> {
    let Some(ticket) = state
        .harness
        .store()
        .load_bridge_bootstrap_ticket(&payload.ticket_id)
        .map_err(internal_error)?
    else {
        return Err((StatusCode::UNAUTHORIZED, "bootstrap ticket not found".into()));
    };
    if ticket.status != BridgeCredentialStatus::PendingBootstrap {
        issue_bootstrap_ticket(&state.harness, state.bridge_instance_id.as_str(), &payload.adapter_id)
            .map_err(internal_error)?;
        return Err((
            StatusCode::UNAUTHORIZED,
            "bootstrap ticket already consumed; refresh bootstrap material and retry".into(),
        ));
    }
    if ticket.bridge_instance_id != payload.bridge_instance_id
        || payload.bridge_instance_id != *state.bridge_instance_id
    {
        return Err((StatusCode::UNAUTHORIZED, "bridge instance mismatch".into()));
    }
    if ticket.adapter_id != payload.adapter_id {
        return Err((StatusCode::UNAUTHORIZED, "adapter mismatch".into()));
    }
    if !state
        .harness
        .store()
        .bridge_ticket_matches_secret(&ticket, &payload.ticket_secret)
    {
        state
            .harness
            .store()
            .append_bridge_auth_audit(
                "bridge.bootstrap.rejected",
                json!({
                    "ticket_id": payload.ticket_id,
                    "reason": "ticket_secret_mismatch",
                }),
            )
            .map_err(internal_error)?;
        return Err((StatusCode::UNAUTHORIZED, "bootstrap secret mismatch".into()));
    }

    let session_secret = new_id("session-secret");
    let rotation_epoch = state
        .harness
        .store()
        .next_bridge_rotation_epoch(state.bridge_instance_id.as_str(), &payload.adapter_id)
        .map_err(internal_error)?;
    let credential = BridgeSessionCredential {
        credential_id: new_id("credential"),
        bridge_instance_id: (*state.bridge_instance_id).clone(),
        adapter_id: payload.adapter_id.clone(),
        secret_ref: new_id("secret-ref"),
        secret_hash: state.harness.store().hash_bridge_secret(&session_secret),
        rotation_epoch,
        issued_at: now_timestamp(),
        expires_at: None,
        status: BridgeCredentialStatus::Active,
    };
    state
        .harness
        .store()
        .consume_bridge_bootstrap_ticket(&ticket.ticket_id)
        .map_err(internal_error)?;
    if credential.rotation_epoch > 1 {
        state
            .harness
            .store()
            .revoke_bridge_credentials_for_instance(state.bridge_instance_id.as_str())
            .map_err(internal_error)?;
        state
            .harness
            .store()
            .append_bridge_auth_audit(
                "bridge.credential.revoked",
                json!({
                "bridge_instance_id": state.bridge_instance_id.as_str(),
                    "adapter_id": payload.adapter_id.as_str(),
                    "rotation_epoch": credential.rotation_epoch,
                }),
            )
            .map_err(internal_error)?;
        log_bridge_event(
            "bridge.credential.revoked",
            json!({
                "bridge_instance_id": state.bridge_instance_id.as_str(),
                "adapter_id": payload.adapter_id.as_str(),
                "rotation_epoch": credential.rotation_epoch,
            }),
        );
    }
    state
        .harness
        .store()
        .save_bridge_session_credential(&credential)
        .map_err(internal_error)?;
    state
        .harness
        .store()
        .append_bridge_auth_audit(
            "bridge.bootstrap.accepted",
            json!({
                "bridge_instance_id": state.bridge_instance_id.as_str(),
                "credential_id": credential.credential_id,
                "secret_ref": credential.secret_ref,
            }),
        )
        .map_err(internal_error)?;
    log_bridge_event(
        "bridge.bootstrap.accepted",
        json!({
            "bridge_instance_id": state.bridge_instance_id.as_str(),
            "credential_id": credential.credential_id,
            "secret_ref": credential.secret_ref,
        }),
    );
    state
        .session_secrets
        .lock()
        .map_err(|_| internal_error(anyhow::anyhow!("session secret mutex poisoned")))?
        .insert(credential.secret_ref.clone(), session_secret.clone());
    issue_bootstrap_ticket(&state.harness, state.bridge_instance_id.as_str(), &payload.adapter_id)
        .map_err(internal_error)?;
    Ok(Json(BridgeBootstrapAck {
        bridge_instance_id: (*state.bridge_instance_id).clone(),
        credential_id: credential.credential_id,
        secret_ref: credential.secret_ref,
        rotation_epoch: credential.rotation_epoch,
        session_secret,
        issued_at: credential.issued_at,
        expires_at: credential.expires_at,
    }))
}

async fn ingest_current_turn(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload: CurrentTurnEnvelope = parse_authenticated_json(&state, request).await?;
    state
        .harness
        .ingest_current_turn(payload)
        .map(|_| StatusCode::ACCEPTED)
        .map_err(internal_error)
}

async fn ingest_capability_snapshot(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload: HostCapabilitySnapshot = parse_authenticated_json(&state, request).await?;
    state
        .harness
        .ingest_capability_snapshot(payload)
        .map(|_| StatusCode::ACCEPTED)
        .map_err(internal_error)
}

async fn ingest_semantic_decision(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload: SemanticDecisionEnvelope = parse_authenticated_json(&state, request).await?;
    state
        .harness
        .ingest_semantic_decision(payload)
        .map(|_| StatusCode::ACCEPTED)
        .map_err(internal_error)
}

async fn ingest_control_action(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload: ControlAction = parse_authenticated_json(&state, request).await?;
    state
        .harness
        .ingest_control_action(payload)
        .map(|_| StatusCode::ACCEPTED)
        .map_err(|error| {
            if error
                .downcast_ref::<loom_harness::LoomHarnessError>()
                .is_some()
            {
                (StatusCode::BAD_REQUEST, error.to_string())
            } else {
                internal_error(error)
            }
        })
}

async fn ingest_subagent_lifecycle(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload: HostSubagentLifecycleEnvelope = parse_authenticated_json(&state, request).await?;
    state
        .harness
        .ingest_subagent_lifecycle(payload)
        .map(|_| StatusCode::ACCEPTED)
        .map_err(internal_error)
}

async fn get_next_outbound(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<Json<OutboundDelivery>, (StatusCode, String)> {
    let (parts, body) = request.into_parts();
    verify_auth(
        &state,
        parts.method.as_str(),
        parts
            .uri
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or(parts.uri.path()),
        &parts.headers,
        parts.extensions.get::<SocketAddr>().copied(),
        &body_to_bytes(body).await?,
    )?;
    let host_session_id = parse_host_session_id(
        parts
            .uri
            .query()
            .ok_or((StatusCode::BAD_REQUEST, "missing host_session_id".into()))?,
    )?;
    state
        .harness
        .store()
        .next_outbound(&host_session_id)
        .map_err(internal_error)?
        .map(Json)
        .ok_or((StatusCode::NO_CONTENT, "no pending outbound".into()))
}

async fn ack_outbound(
    State(state): State<BridgeState>,
    Path(delivery_id): Path<String>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let (parts, body) = request.into_parts();
    verify_auth(
        &state,
        parts.method.as_str(),
        parts
            .uri
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or(parts.uri.path()),
        &parts.headers,
        parts.extensions.get::<SocketAddr>().copied(),
        &body_to_bytes(body).await?,
    )?;
    let acknowledged = state
        .harness
        .store()
        .ack_outbound(&delivery_id)
        .map_err(internal_error)?;
    if acknowledged {
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, "delivery not found".into()))
    }
}

async fn get_next_host_execution(
    State(state): State<BridgeState>,
    request: Request,
) -> Result<Json<HostExecutionCommand>, (StatusCode, String)> {
    let (parts, body) = request.into_parts();
    verify_auth(
        &state,
        parts.method.as_str(),
        parts
            .uri
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or(parts.uri.path()),
        &parts.headers,
        parts.extensions.get::<SocketAddr>().copied(),
        &body_to_bytes(body).await?,
    )?;
    let host_session_id = parse_host_session_id(
        parts
            .uri
            .query()
            .ok_or((StatusCode::BAD_REQUEST, "missing host_session_id".into()))?,
    )?;
    state
        .harness
        .store()
        .next_host_execution_command(&host_session_id)
        .map_err(internal_error)?
        .map(Json)
        .ok_or((StatusCode::NO_CONTENT, "no pending host execution command".into()))
}

async fn ack_host_execution(
    State(state): State<BridgeState>,
    Path(command_id): Path<String>,
    request: Request,
) -> Result<StatusCode, (StatusCode, String)> {
    let (parts, body) = request.into_parts();
    verify_auth(
        &state,
        parts.method.as_str(),
        parts
            .uri
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or(parts.uri.path()),
        &parts.headers,
        parts.extensions.get::<SocketAddr>().copied(),
        &body_to_bytes(body).await?,
    )?;
    let acknowledged = state
        .harness
        .store()
        .ack_host_execution_command(&command_id)
        .map_err(internal_error)?;
    if acknowledged {
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, "command not found".into()))
    }
}

async fn parse_authenticated_json<T: DeserializeOwned>(
    state: &BridgeState,
    request: Request,
) -> Result<T, (StatusCode, String)> {
    let (parts, body) = request.into_parts();
    let path = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(parts.uri.path())
        .to_string();
    let bytes = body_to_bytes(body).await?;
    verify_auth(
        state,
        parts.method.as_str(),
        &path,
        &parts.headers,
        parts.extensions.get::<SocketAddr>().copied(),
        &bytes,
    )?;
    serde_json::from_slice::<T>(&bytes)
        .map_err(|error| (StatusCode::BAD_REQUEST, format!("invalid json: {error}")))
}

async fn body_to_bytes(body: Body) -> Result<Vec<u8>, (StatusCode, String)> {
    to_bytes(body, usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| (StatusCode::BAD_REQUEST, format!("invalid body: {error}")))
}

fn parse_host_session_id(query: &str) -> Result<HostSessionId, (StatusCode, String)> {
    query
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key == "host_session_id" {
                Some(percent_decode(value))
            } else {
                None
            }
        })
        .ok_or((StatusCode::BAD_REQUEST, "missing host_session_id".into()))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    index += 3;
                    continue;
                }
                decoded.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn parse_auth_envelope(headers: &HeaderMap) -> Result<BridgeAuthEnvelope, (StatusCode, String)> {
    let header = |name: &str| -> Result<String, (StatusCode, String)> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string())
            .ok_or((StatusCode::UNAUTHORIZED, format!("missing auth header: {name}")))
    };
    Ok(BridgeAuthEnvelope {
        bridge_instance_id: header("x-loom-bridge-instance-id")?,
        adapter_id: header("x-loom-adapter-id")?,
        secret_ref: header("x-loom-secret-ref")?,
        rotation_epoch: header("x-loom-rotation-epoch")?
            .parse::<u32>()
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid rotation epoch".into()))?,
        signed_at: header("x-loom-signed-at")?,
        nonce: header("x-loom-nonce")?,
        signature: header("x-loom-signature")?,
    })
}

fn verify_auth(
    state: &BridgeState,
    method: &str,
    path: &str,
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    body: &[u8],
) -> Result<(), (StatusCode, String)> {
    if let Some(peer_addr) = peer_addr {
        if !peer_addr.ip().is_loopback() {
            return Err(reject_auth(
                state,
                StatusCode::UNAUTHORIZED,
                path,
                "bridge only accepts loopback peers",
                None,
            ));
        }
    }
    let envelope = parse_auth_envelope(headers).map_err(|(status, reason)| {
        reject_auth(state, status, path, &reason, None)
    })?;
    if envelope.bridge_instance_id != *state.bridge_instance_id {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "bridge instance mismatch",
            Some(&envelope),
        ));
    }
    let Some(credential) = state
        .harness
        .store()
        .find_bridge_session_credential_by_secret_ref(&envelope.secret_ref)
        .map_err(internal_error)?
    else {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "session credential not found",
            Some(&envelope),
        ));
    };
    if credential.status != BridgeCredentialStatus::Active {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "session credential is not active",
            Some(&envelope),
        ));
    }
    if credential.adapter_id != envelope.adapter_id {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "adapter id mismatch",
            Some(&envelope),
        ));
    }
    if credential.rotation_epoch != envelope.rotation_epoch {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "rotation epoch mismatch",
            Some(&envelope),
        ));
    }
    let inserted = state
        .harness
        .store()
        .record_bridge_nonce(&envelope.secret_ref, &envelope.nonce, &envelope.signed_at)
        .map_err(internal_error)?;
    if !inserted {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "nonce replay detected",
            Some(&envelope),
        ));
    }
    let body_sha = format!("{:x}", Sha256::digest(body));
    let canonical = [method, path, &body_sha, &envelope.signed_at, &envelope.nonce].join("\n");
    let session_secret = state
        .session_secrets
        .lock()
        .map_err(|_| internal_error(anyhow::anyhow!("session secret mutex poisoned")))?
        .get(&envelope.secret_ref)
        .cloned()
        .ok_or_else(|| {
            reject_auth(
                state,
                StatusCode::UNAUTHORIZED,
                path,
                "session secret unavailable",
                Some(&envelope),
            )
        })?;
    let mut mac = Hmac::<Sha256>::new_from_slice(session_secret.as_bytes())
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "invalid hmac secret".into()))?;
    mac.update(canonical.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    if signature != envelope.signature {
        return Err(reject_auth(
            state,
            StatusCode::UNAUTHORIZED,
            path,
            "signature mismatch",
            Some(&envelope),
        ));
    }
    Ok(())
}

fn reject_auth(
    state: &BridgeState,
    status: StatusCode,
    path: &str,
    reason: &str,
    envelope: Option<&BridgeAuthEnvelope>,
) -> (StatusCode, String) {
    let payload = json!({
        "path": path,
        "reason": reason,
        "bridge_instance_id": envelope.map(|value| value.bridge_instance_id.clone()),
        "adapter_id": envelope.map(|value| value.adapter_id.clone()),
        "secret_ref": envelope.map(|value| value.secret_ref.clone()),
        "rotation_epoch": envelope.map(|value| value.rotation_epoch),
    });
    let _ = state
        .harness
        .store()
        .append_bridge_auth_audit("bridge.auth.failed", payload);
    (status, reason.into())
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

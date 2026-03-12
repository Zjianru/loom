use loom_domain::{
    DeliveryStatus, KernelOutboundPayload, ManagedTaskClass, RenderEmphasis, RenderHint,
    RenderTone, StartCardAction, StartCardPayload, StatusNoticeKind, StatusNoticePayload,
    TaskActivationReason, WorkHorizonKind, now_timestamp,
};
use loom_store::LoomStore;
use tempfile::tempdir;

fn store() -> LoomStore {
    let dir = tempdir().expect("tempdir");
    LoomStore::in_memory(dir.keep()).expect("store")
}

fn start_card() -> KernelOutboundPayload {
    KernelOutboundPayload::StartCard(StartCardPayload {
        managed_task_ref: "task-1".into(),
        decision_token: "decision-1".into(),
        managed_task_class: ManagedTaskClass::Complex,
        work_horizon: WorkHorizonKind::Maintenance,
        task_activation_reason: TaskActivationReason::ExplicitStartTask,
        title: "Managed task".into(),
        summary: "Retry delivery".into(),
        expected_outcome: "Visible start card".into(),
        recommended_pack_ref: Some("coding_pack".into()),
        allowed_actions: vec![
            StartCardAction::ApproveStart,
            StartCardAction::ModifyCandidate,
            StartCardAction::CancelCandidate,
        ],
        render_hint: Default::default(),
    })
}

fn status_notice() -> KernelOutboundPayload {
    KernelOutboundPayload::StatusNotice(StatusNoticePayload {
        managed_task_ref: "task-1".into(),
        notice_kind: StatusNoticeKind::StageEntered,
        stage_ref: "phase-entry-execute".into(),
        headline: "Entered execute stage".into(),
        summary: "Task entered execute and queued worker dispatch.".into(),
        detail: Some("Worker dispatch has been queued through host execution.".into()),
        render_hint: RenderHint {
            tone: RenderTone::Neutral,
            emphasis: RenderEmphasis::Minimal,
            ..RenderHint::default()
        },
    })
}

#[test]
fn status_notice_delivery_keeps_managed_task_ref_for_outbox_queries() {
    let store = store();
    let delivery = store
        .enqueue_outbound("session-1".into(), status_notice())
        .expect("enqueue status notice");

    let stored = store
        .load_outbound(&delivery.delivery_id)
        .expect("load status notice")
        .expect("status notice exists");
    assert_eq!(stored.managed_task_ref.as_deref(), Some("task-1"));
    let loom_domain::KernelOutboundPayload::StatusNotice(status_notice) = stored.payload else {
        panic!("expected status notice payload");
    };
    assert_eq!(status_notice.stage_ref, "phase-entry-execute");
}

#[test]
fn retry_scheduled_delivery_waits_until_next_attempt_at() {
    let store = store();
    let delivery = store
        .enqueue_outbound("session-1".into(), start_card())
        .expect("enqueue outbound");
    let fetched = store
        .next_outbound(&"session-1".to_string())
        .expect("next outbound")
        .expect("delivery");
    assert_eq!(fetched.delivery_id, delivery.delivery_id);
    assert_eq!(fetched.attempts, 1);

    let future_attempt_at = (now_timestamp().parse::<u128>().expect("millis") + 60_000).to_string();
    assert!(
        store
            .schedule_outbound_retry(
                &delivery.delivery_id,
                future_attempt_at.clone(),
                "transcript file not found".into(),
            )
            .expect("schedule retry")
    );

    let stored = store
        .load_outbound(&delivery.delivery_id)
        .expect("load delivery")
        .expect("delivery exists");
    assert_eq!(stored.delivery_status, DeliveryStatus::RetryScheduled);
    assert_eq!(
        stored.next_attempt_at.as_deref(),
        Some(future_attempt_at.as_str())
    );
    assert_eq!(
        stored.last_error.as_deref(),
        Some("transcript file not found")
    );
    assert!(
        store
            .next_outbound(&"session-1".to_string())
            .expect("next outbound after retry")
            .is_none()
    );
}

#[test]
fn retry_scheduled_delivery_redelivers_when_due_and_ends_terminally_at_max_attempts() {
    let store = store();
    let delivery = store
        .enqueue_outbound("session-1".into(), start_card())
        .expect("enqueue outbound");

    for attempt in 1..delivery.max_attempts {
        let outbound = store
            .next_outbound(&"session-1".to_string())
            .expect("outbound during retry window")
            .expect("delivery");
        assert_eq!(outbound.attempts, attempt);
        assert_eq!(outbound.delivery_status, DeliveryStatus::Delivering);
        assert!(
            store
                .schedule_outbound_retry(
                    &delivery.delivery_id,
                    "0".into(),
                    format!("failure-{attempt}"),
                )
                .expect("schedule retry")
        );

        let stored = store
            .load_outbound(&delivery.delivery_id)
            .expect("load retry-scheduled delivery")
            .expect("delivery exists");
        assert_eq!(
            stored.delivery_status,
            if attempt < delivery.max_attempts {
                DeliveryStatus::RetryScheduled
            } else {
                DeliveryStatus::TerminalFailed
            }
        );
    }

    let final_attempt = store
        .next_outbound(&"session-1".to_string())
        .expect("final outbound")
        .expect("final delivery");
    assert_eq!(final_attempt.attempts, delivery.max_attempts);
    assert_eq!(final_attempt.delivery_status, DeliveryStatus::Delivering);
    assert!(
        store
            .schedule_outbound_retry(&delivery.delivery_id, "0".into(), "final failure".into())
            .expect("mark terminal")
    );

    let terminal = store
        .load_outbound(&delivery.delivery_id)
        .expect("load terminal delivery")
        .expect("delivery exists");
    assert_eq!(terminal.delivery_status, DeliveryStatus::TerminalFailed);
    assert_eq!(terminal.next_attempt_at, None);
    assert_eq!(terminal.last_error.as_deref(), Some("final failure"));
    assert!(
        store
            .next_outbound(&"session-1".to_string())
            .expect("no outbound after terminal")
            .is_none()
    );
}

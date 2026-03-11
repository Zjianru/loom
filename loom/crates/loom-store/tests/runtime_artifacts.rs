use loom_domain::{
    AcceptanceResult, AgentBinding, AgentBindingMember, AgentBindingStatus, AgentExecutionMode,
    AgentRoleKind, ArtifactManifestItem, HostExecutionCommand, HostExecutionCommandStatus,
    HostSubagentLifecycleEnvelope, HostSubagentLifecycleEvent, NextActionItem, PhaseEntryOrigin,
    PhasePlan, PhasePlanEntry, PhasePlanMetadata, PhasePlanMutationPolicy, PhasePlanSource,
    ProofEvidenceRef, ProofOfWorkBundle, ResultContract, ResultOutcome, ReviewResult,
    ReviewSummary, ReviewVerdict, StageVisibility, SubagentSpawnedPayload, now_timestamp,
};
use loom_store::LoomStore;
use tempfile::tempdir;

fn store() -> LoomStore {
    let dir = tempdir().expect("tempdir");
    LoomStore::in_memory(dir.keep()).expect("store")
}

#[test]
fn task_artifacts_roundtrip_through_sqlite() {
    let store = store();
    let phase_plan = PhasePlan {
        phase_plan_id: "phase-plan-1".into(),
        managed_task_ref: "task-1".into(),
        plan_source: PhasePlanSource::SystemDefault,
        plan_entries: vec![PhasePlanEntry {
            entry_id: "phase-entry-1".into(),
            stage_package_id: "execute".into(),
            sequence_no: 1,
            visibility: StageVisibility::UserVisible,
            origin: PhaseEntryOrigin::PackDefault,
            required: true,
            skip_allowed: false,
            rework_target: None,
        }],
        mutation_policy: PhasePlanMutationPolicy {
            user_adjustment_allowed: false,
            system_insert_allowed: true,
        },
        metadata: PhasePlanMetadata {
            pack_ref: Some("coding_pack".into()),
            default_stage_sequence: vec!["execute".into(), "review".into(), "deliver".into()],
        },
        created_at: now_timestamp(),
    };
    let binding = AgentBinding {
        binding_id: "binding-1".into(),
        managed_task_ref: "task-1".into(),
        run_ref: "run-1".into(),
        stage_run_ref: None,
        pack_ref: Some("coding_pack".into()),
        capability_snapshot_ref: "cap-1".into(),
        members: vec![AgentBindingMember {
            role_kind: AgentRoleKind::Worker,
            profile_ref: "coder".into(),
            host_mapping_ref: Some("coder".into()),
            responsibilities: vec!["Implement the task".into()],
            execution_mode: AgentExecutionMode::BackgroundWorker,
            required: true,
        }],
        status: AgentBindingStatus::Active,
        issued_reason: "test".into(),
        issued_at: now_timestamp(),
        supersedes: None,
    };
    let review = ReviewResult {
        review_result_id: "review-1".into(),
        managed_task_ref: "task-1".into(),
        run_ref: "run-1".into(),
        reviewer_group_ref: "system.review.v0".into(),
        review_verdict: ReviewVerdict::Approved,
        findings: Vec::new(),
        review_artifacts: vec![ArtifactManifestItem {
            label: "artifact".into(),
            reference: "/tmp/artifact.txt".into(),
        }],
        summary: ReviewSummary {
            review_verdict: ReviewVerdict::Approved,
            summary: "Review approved the worker output.".into(),
            key_findings: Vec::new(),
            follow_up_required: false,
        },
        reviewed_at: now_timestamp(),
    };
    let proof = ProofOfWorkBundle {
        proof_of_work_id: "proof-1".into(),
        managed_task_ref: "task-1".into(),
        run_ref: "run-1".into(),
        acceptance_verdict: AcceptanceResult::Accepted,
        acceptance_basis: vec!["Review approved.".into()],
        accepted_by_ref: "system.review.v0".into(),
        accepted_at: now_timestamp(),
        accepted_scope_version: 1,
        review_summary: review.summary.clone(),
        artifact_manifest: review.review_artifacts.clone(),
        evidence_refs: vec![ProofEvidenceRef {
            label: "run_ref".into(),
            reference: "run-1".into(),
        }],
        run_summary: "worker/review/recorder completed".into(),
    };
    let contract = ResultContract {
        result_contract_id: "result-1".into(),
        managed_task_ref: "task-1".into(),
        pack_ref: Some("coding_pack".into()),
        outcome: ResultOutcome::Completed,
        acceptance_verdict: AcceptanceResult::Accepted,
        final_scope_version: 1,
        scope_revision_summary: Some("scope_version=1".into()),
        summary: "First-round closed loop completed.".into(),
        key_outcomes: vec!["Review approved.".into()],
        proof_of_work: proof.clone(),
        next_actions: vec![NextActionItem {
            title: "Verify summary".into(),
            details: "Confirm the final result card.".into(),
        }],
        created_at: now_timestamp(),
    };

    store.save_phase_plan(&phase_plan).expect("save phase plan");
    store.save_agent_binding(&binding).expect("save binding");
    store.save_review_result(&review).expect("save review");
    store
        .save_proof_of_work_bundle(&proof)
        .expect("save proof of work");
    store
        .save_result_contract(&contract)
        .expect("save result contract");

    assert_eq!(
        store
            .latest_phase_plan(&"task-1".to_string())
            .expect("phase plan")
            .expect("phase plan exists")
            .phase_plan_id,
        "phase-plan-1"
    );
    assert_eq!(
        store
            .latest_agent_binding(&"task-1".to_string())
            .expect("binding")
            .expect("binding exists")
            .binding_id,
        "binding-1"
    );
    assert_eq!(
        store
            .latest_review_result(&"task-1".to_string())
            .expect("review")
            .expect("review exists")
            .review_result_id,
        "review-1"
    );
    assert_eq!(
        store
            .latest_proof_of_work_bundle(&"task-1".to_string())
            .expect("proof")
            .expect("proof exists")
            .proof_of_work_id,
        "proof-1"
    );
    assert_eq!(
        store
            .latest_result_contract(&"task-1".to_string())
            .expect("result")
            .expect("result exists")
            .result_contract_id,
        "result-1"
    );
}

#[test]
fn host_execution_queue_ack_and_lifecycle_are_durable() {
    let store = store();
    let command = HostExecutionCommand {
        command_id: "exec-1".into(),
        managed_task_ref: "task-1".into(),
        run_ref: "run-1".into(),
        binding_id: "binding-1".into(),
        role_kind: AgentRoleKind::Worker,
        host_session_id: "agent:main:main".into(),
        host_agent_id: "coder".into(),
        prompt: "Implement the task".into(),
        label: "loom-worker-task-1".into(),
        status: HostExecutionCommandStatus::Pending,
        host_child_execution_ref: None,
        host_child_run_ref: None,
        output_summary: None,
        artifact_refs: Vec::new(),
        issued_at: now_timestamp(),
        acked_at: None,
        completed_at: None,
    };
    store
        .enqueue_host_execution_command(&command)
        .expect("enqueue command");

    assert_eq!(
        store
            .next_host_execution_command(&"agent:main:main".to_string())
            .expect("next command")
            .expect("command exists")
            .command_id,
        "exec-1"
    );
    assert!(store.ack_host_execution_command(&"exec-1".to_string()).expect("ack command"));
    assert!(
        store
            .next_host_execution_command(&"agent:main:main".to_string())
            .expect("next command after ack")
            .is_none()
    );

    let lifecycle = HostSubagentLifecycleEnvelope {
        meta: loom_domain::IngressMeta::default(),
        command_id: "exec-1".into(),
        managed_task_ref: "task-1".into(),
        run_ref: "run-1".into(),
        role_kind: AgentRoleKind::Worker,
        event: HostSubagentLifecycleEvent::Spawned(SubagentSpawnedPayload {
            host_child_execution_ref: "agent:coder:child-1".into(),
            host_child_run_ref: Some("run-child-1".into()),
            host_agent_id: "coder".into(),
            observed_at: now_timestamp(),
        }),
    };
    store
        .save_host_subagent_lifecycle_event(&lifecycle)
        .expect("save lifecycle");

    let lifecycle_events = store
        .list_host_subagent_lifecycle_events(&"task-1".to_string())
        .expect("list lifecycle");
    assert_eq!(lifecycle_events.len(), 1);
    assert_eq!(lifecycle_events[0].command_id, "exec-1");
}

#[test]
fn host_execution_late_ack_backfills_timestamp_without_downgrading_status() {
    let store = store();
    let command = HostExecutionCommand {
        command_id: "exec-late-ack".into(),
        managed_task_ref: "task-1".into(),
        run_ref: "run-1".into(),
        binding_id: "binding-1".into(),
        role_kind: AgentRoleKind::Worker,
        host_session_id: "agent:main:main".into(),
        host_agent_id: "coder".into(),
        prompt: "Implement the task".into(),
        label: "loom-worker-task-1".into(),
        status: HostExecutionCommandStatus::Running,
        host_child_execution_ref: Some("agent:coder:child-1".into()),
        host_child_run_ref: Some("run-child-1".into()),
        output_summary: None,
        artifact_refs: Vec::new(),
        issued_at: now_timestamp(),
        acked_at: None,
        completed_at: None,
    };
    store
        .save_host_execution_command(&command)
        .expect("save command");

    assert!(
        store
            .ack_host_execution_command(&"exec-late-ack".to_string())
            .expect("late ack")
    );

    let command_after = store
        .load_host_execution_command(&"exec-late-ack".to_string())
        .expect("load command")
        .expect("command exists");
    assert_eq!(command_after.status, HostExecutionCommandStatus::Running);
    assert!(command_after.acked_at.is_some());
}

#[test]
fn host_execution_transport_legacy_aliases_deserialize_into_canonical_fields() {
    let command: HostExecutionCommand = serde_json::from_value(serde_json::json!({
        "command_id": "exec-legacy",
        "managed_task_ref": "task-1",
        "run_ref": "run-1",
        "binding_id": "binding-1",
        "role_kind": "worker",
        "host_session_id": "agent:main:main",
        "host_agent_id": "coder",
        "prompt": "Implement the task",
        "label": "loom-worker-task-1",
        "status": "pending",
        "child_session_key": "agent:coder:child-1",
        "child_run_id": "run-child-1",
        "output_summary": null,
        "artifact_refs": [],
        "issued_at": "1000",
        "acked_at": null,
        "completed_at": null
    }))
    .expect("deserialize legacy command");
    assert_eq!(
        command.host_child_execution_ref.as_deref(),
        Some("agent:coder:child-1")
    );
    assert_eq!(command.host_child_run_ref.as_deref(), Some("run-child-1"));

    let lifecycle: HostSubagentLifecycleEnvelope = serde_json::from_value(serde_json::json!({
        "meta": {
            "ingress_id": "ingress-1",
            "received_at": "1000",
            "causation_id": null,
            "correlation_id": "corr-1",
            "dedupe_window": "PT10M"
        },
        "command_id": "exec-legacy",
        "managed_task_ref": "task-1",
        "run_ref": "run-1",
        "role_kind": "worker",
        "event": {
            "spawned": {
                "child_session_key": "agent:coder:child-1",
                "child_run_id": "run-child-1",
                "host_agent_id": "coder",
                "observed_at": "1001"
            }
        }
    }))
    .expect("deserialize legacy lifecycle");
    match lifecycle.event {
        HostSubagentLifecycleEvent::Spawned(payload) => {
            assert_eq!(payload.host_child_execution_ref, "agent:coder:child-1");
            assert_eq!(payload.host_child_run_ref.as_deref(), Some("run-child-1"));
        }
        _ => panic!("expected spawned payload"),
    }
}

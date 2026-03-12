export type ManagedTaskClass = "COMPLEX" | "HUGE" | "MAX";
export type WireManagedTaskClass = "complex" | "huge" | "max";
export type HostSessionId = string;
export type WorkHorizon =
  | "maintenance"
  | "improvement"
  | "extension"
  | "disruption";
export type InteractionLane =
  | "chat"
  | "managed_task_candidate"
  | "managed_task_active";
export type TaskActivationReason =
  | "explicit_start_task"
  | "explicit_track_this"
  | "delegate_heavy_work"
  | "heavy_multi_stage_goal";
export type TaskChangeClassification =
  | "same_task_minor"
  | "same_task_material"
  | "same_task_structural"
  | "boundary_conflict_candidate";
export type ChangeExecutionSurface = "future_only" | "active_stage" | "completed_scope";
export type BoundaryRecommendation =
  | "absorb_change"
  | "require_confirmation"
  | "open_boundary_confirmation";
export type DecisionSource =
  | "host_model"
  | "pack_default"
  | "system_reconsideration"
  | "user_control_action"
  | "adapter_fallback";
export type HostSemanticDecisionKind =
  | "interaction_lane"
  | "task_activation_reason"
  | "managed_task_class"
  | "work_horizon"
  | "task_change"
  | "control_action";
export type ControlActionKind =
  | "approve_start"
  | "modify_candidate"
  | "cancel_candidate"
  | "keep_current_task"
  | "replace_active"
  | "approve_request"
  | "reject_request"
  | "pause_task"
  | "resume_task"
  | "cancel_task"
  | "request_review"
  | "request_horizon_reconsideration"
  | "request_task_change";

export type HostSemanticSchemaVersion = {
  major: number;
  minor: number;
};

export type RequirementItemDraft = {
  text: string;
  origin?: string;
};

export type HostTaskShape = {
  managed_task_ref?: string;
  title?: string;
  summary?: string;
  expected_outcome?: string;
  requirement_items?: RequirementItemDraft[];
  workspace_ref?: string;
  repo_ref?: string;
  allowed_roots?: string[];
  secret_classes?: string[];
};

export type HostSemanticDecision =
  | {
      decision_ref: string;
      decision_kind: "interaction_lane";
      decision_source: DecisionSource;
      confidence: number;
      rationale: string;
      payload: HostTaskShape & { interaction_lane: InteractionLane };
    }
  | {
      decision_ref: string;
      decision_kind: "task_activation_reason";
      decision_source: DecisionSource;
      confidence: number;
      rationale: string;
      payload: { task_activation_reason: TaskActivationReason };
    }
  | {
      decision_ref: string;
      decision_kind: "managed_task_class";
      decision_source: DecisionSource;
      confidence: number;
      rationale: string;
      payload: { managed_task_class: WireManagedTaskClass };
    }
  | {
      decision_ref: string;
      decision_kind: "work_horizon";
      decision_source: DecisionSource;
      confidence: number;
      rationale: string;
      payload: { work_horizon: WorkHorizon };
    }
  | {
      decision_ref: string;
      decision_kind: "task_change";
      decision_source: DecisionSource;
      confidence: number;
      rationale: string;
      payload: {
        classification: TaskChangeClassification;
        execution_surface: ChangeExecutionSurface;
        boundary_recommendation: BoundaryRecommendation;
      };
    }
  | {
      decision_ref: string;
      decision_kind: "control_action";
      decision_source: DecisionSource;
      confidence: number;
      rationale: string;
      payload: {
        action_kind: ControlActionKind;
        decision_token?: string;
        managed_task_ref?: string;
        source_decision_ref?: string;
        payload?: {
          title?: string;
          summary?: string;
          expected_outcome?: string;
          requirement_items?: RequirementItemDraft[];
          allowed_roots?: string[];
          secret_classes?: string[];
          workspace_ref?: string;
          repo_ref?: string;
          rationale?: string;
        };
      };
    };

export type HostSemanticBundle = {
  schema_version: HostSemanticSchemaVersion;
  input_ref: string;
  source_model_ref: string;
  issued_at: string;
  decisions: HostSemanticDecision[];
  rationale_summary?: string;
};

export type SemanticDecisionKind =
  | "interaction_lane"
  | "task_activation_reason"
  | "managed_task_class"
  | "work_horizon"
  | "task_change";

export type InteractionLaneDecisionPayload = HostTaskShape & {
  interaction_lane: InteractionLane;
};

export type TaskActivationReasonDecisionPayload = {
  task_activation_reason: TaskActivationReason;
};

export type ManagedTaskClassDecisionPayload = {
  managed_task_class: ManagedTaskClass;
};

export type WorkHorizonDecisionPayload = {
  work_horizon: WorkHorizon;
};

export type TaskChangeDecisionPayload = {
  classification: TaskChangeClassification;
  execution_surface: ChangeExecutionSurface;
  boundary_recommendation: BoundaryRecommendation;
};

export type SemanticDecisionPayload =
  | InteractionLaneDecisionPayload
  | TaskActivationReasonDecisionPayload
  | ManagedTaskClassDecisionPayload
  | WorkHorizonDecisionPayload
  | TaskChangeDecisionPayload;

export type CurrentTurnEnvelope = {
  meta: {
    ingress_id: string;
    received_at: string;
    causation_id?: string | null;
    correlation_id: string;
    dedupe_window: string;
  };
  host_session_id: string;
  host_message_ref?: string | null;
  text: string;
  workspace_ref?: string | null;
  repo_ref?: string | null;
};

export type SemanticDecisionEnvelope = {
  decision_ref: string;
  host_session_id: string;
  host_message_ref?: string | null;
  managed_task_ref?: string | null;
  decision_kind: SemanticDecisionKind;
  decision_source: DecisionSource;
  rationale: string;
  confidence: number;
  source_model_ref: string;
  issued_at: string;
  decision_payload: SemanticDecisionPayload;
};

export type ControlAction = {
  action_id: string;
  managed_task_ref?: string | null;
  kind: ControlActionKind;
  actor: "user";
  payload: {
    title?: string | null;
    summary?: string | null;
    expected_outcome?: string | null;
    requirement_items: Array<{ text: string; origin: string }>;
    allowed_roots: string[];
    secret_classes: string[];
    workspace_ref?: string | null;
    repo_ref?: string | null;
    rationale?: string | null;
  };
  source_decision_ref?: string | null;
  decision_token?: string | null;
};

export type ControlActionEnvelope = {
  decision_ref: string;
  decision_source: DecisionSource;
  rationale: string;
  confidence: number;
  source_model_ref: string;
  issued_at: string;
  action: ControlAction;
};

export type SemanticDecisionBatchEnvelope = {
  meta: {
    ingress_id: string;
    received_at: string;
    causation_id?: string | null;
    correlation_id: string;
    dedupe_window: string;
  };
  host_session_id: string;
  host_message_ref?: string | null;
  input_ref: string;
  source_model_ref: string;
  issued_at: string;
  rationale_summary?: string | null;
  semantic_decisions: SemanticDecisionEnvelope[];
  control_action?: ControlActionEnvelope | null;
};

export type LegacySemanticDecisionEnvelope = {
  decision_id: string;
  host_session_id: string;
  host_message_ref?: string | null;
  managed_task_ref?: string | null;
  interaction_lane: InteractionLane;
  managed_task_class?: ManagedTaskClass | null;
  work_horizon?: WorkHorizon | null;
  task_activation_reason?: TaskActivationReason | null;
  task_change_classification?: TaskChangeClassification | null;
  task_change_execution_surface?: ChangeExecutionSurface | null;
  task_change_boundary_recommendation?: BoundaryRecommendation | null;
  title?: string | null;
  summary?: string | null;
  expected_outcome?: string | null;
  requirement_items: Array<{ text: string; origin: string }>;
  workspace_ref?: string | null;
  repo_ref?: string | null;
  allowed_roots: string[];
  secret_classes: string[];
  confidence?: number | null;
  created_at: string;
};

export type BridgeBootstrapMaterial = {
  bridge_instance_id: string;
  adapter_id: string;
  ticket_id: string;
  ticket_secret: string;
  issued_at: string;
  expires_at: string;
};

export type BridgeBootstrapRequest = {
  bridge_instance_id: string;
  adapter_id: string;
  ticket_id: string;
  ticket_secret: string;
  requested_at: string;
};

export type BridgeBootstrapAck = {
  bridge_instance_id: string;
  credential_id: string;
  secret_ref: string;
  rotation_epoch: number;
  session_secret: string;
  issued_at: string;
  expires_at?: string | null;
};

export type BridgeHealthResponse = {
  bridge_instance_id: string;
  status: string;
};

export type BridgeSessionCredential = {
  bridge_instance_id: string;
  adapter_id: string;
  secret_ref: string;
  rotation_epoch: number;
  session_secret: string;
};

export type BridgeStatus =
  | "disconnected"
  | "connecting"
  | "bootstrapping"
  | "active"
  | "reconnect_required"
  | "fail_closed";

export type ControlSurfaceType =
  | "start_card"
  | "boundary_card"
  | "approval_request";

export type CurrentControlSurfaceProjection = {
  host_session_id: string;
  surface_type: ControlSurfaceType;
  managed_task_ref: string;
  decision_token: string;
  allowed_actions: ControlActionKind[];
};

export type StartCardPayload = {
  managed_task_ref: string;
  decision_token: string;
  managed_task_class: ManagedTaskClass;
  work_horizon: WorkHorizon;
  task_activation_reason: TaskActivationReason;
  title: string;
  summary: string;
  expected_outcome: string;
  recommended_pack_ref?: string | null;
  allowed_actions: string[];
};

export type ApprovalRequestPayload = {
  managed_task_ref: string;
  decision_token: string;
  approval_scope: string;
  allowed_actions: string[];
  why_now: string;
  risk_summary: string;
};

export type BoundaryCardPayload = {
  managed_task_ref: string;
  candidate_managed_task_ref: string;
  decision_token: string;
  active_task_summary: string;
  candidate_task_summary: string;
  boundary_reason: string;
  allowed_actions: string[];
};

export type ResultSummaryPayload = {
  managed_task_ref: string;
  outcome: string;
  acceptance_verdict: string;
  summary: string;
  final_scope_version: number;
  scope_revision_headline?: string | null;
  proof_of_work_excerpt: {
    run_summary: string;
    evidence_refs: Array<{ label: string; reference: string }>;
    review_summary?: {
      review_verdict: string;
      summary: string;
      key_findings: string[];
      follow_up_required: boolean;
    } | null;
    artifact_manifest_excerpt: Array<{ label: string; reference: string }>;
    acceptance_basis_excerpt: string[];
  };
  next_actions_excerpt: Array<{ title: string; details: string }>;
  render_hint?: {
    tone: string;
    emphasis: string;
    host_text_mode: string;
  };
};

export type SuppressHostMessagePayload = {
  host_message_ref?: string | null;
  reason: string;
};

export type ToolDecisionPayload = {
  managed_task_ref: string;
  decision_value: "allow" | "deny" | "requires_user_approval";
  decision_area: "task_execution" | "tool_execution" | "task_change_confirmation" | "capability_drift_resolution" | "review_resolution";
  summary: string;
};

export type StatusNoticePayload = {
  managed_task_ref: string;
  notice_kind: "stage_entered" | "blocked";
  stage_ref: string;
  headline: string;
  summary: string;
  detail?: string | null;
  render_hint?: {
    tone: string;
    emphasis: string;
    host_text_mode: string;
  };
};

export type KernelOutboundPayload =
  | { type: "start_card"; data: StartCardPayload }
  | { type: "boundary_card"; data: BoundaryCardPayload }
  | { type: "approval_request"; data: ApprovalRequestPayload }
  | { type: "result_summary"; data: ResultSummaryPayload }
  | { type: "suppress_host_message"; data: SuppressHostMessagePayload }
  | { type: "tool_decision"; data: ToolDecisionPayload }
  | { type: "status_notice"; data: StatusNoticePayload };

export type OutboundDelivery = {
  delivery_id: string;
  host_session_id: string;
  managed_task_ref?: string | null;
  correlation_id: string;
  causation_id?: string | null;
  payload: KernelOutboundPayload;
  delivery_status: string;
  attempts: number;
  max_attempts: number;
  next_attempt_at?: string | null;
  expires_at?: string | null;
  last_error?: string | null;
  created_at: string;
  acked_at?: string | null;
};

export type HostExecutionCommand = {
  command_id: string;
  managed_task_ref: string;
  run_ref: string;
  binding_id: string;
  role_kind: "net" | "worker" | "recorder";
  host_session_id: string;
  host_agent_id: string;
  prompt: string;
  label: string;
  status: "pending" | "dispatched" | "running" | "completed" | "failed";
  host_child_execution_ref?: string | null;
  host_child_run_ref?: string | null;
  child_session_key?: string | null;
  child_run_id?: string | null;
  output_summary?: string | null;
  artifact_refs: string[];
  issued_at: string;
  acked_at?: string | null;
  completed_at?: string | null;
};

export type HostSubagentLifecycleEnvelope = {
  meta: {
    ingress_id: string;
    received_at: string;
    causation_id?: string | null;
    correlation_id: string;
    dedupe_window: string;
  };
  command_id: string;
  managed_task_ref: string;
  run_ref: string;
  role_kind: "net" | "worker" | "recorder";
  event:
    | {
        spawned: {
          host_child_execution_ref: string;
          host_child_run_ref?: string | null;
          host_agent_id: string;
          observed_at: string;
        };
      }
    | {
        ended: {
          host_child_execution_ref: string;
          host_child_run_ref?: string | null;
          host_agent_id: string;
          status: "completed" | "failed" | "timed_out" | "cancelled";
          output_summary: string;
          artifact_refs: string[];
          observed_at: string;
        };
      };
};

export type TurnBinding = {
  hostSessionId: string;
  hostMessageRef?: string;
  ingressId: string;
  correlationId: string;
  receivedAt: string;
  text: string;
};

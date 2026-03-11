import type {
  ControlAction,
  ControlActionKind,
  HostSemanticBundle,
  InteractionLane,
  ManagedTaskClass,
  SemanticDecisionEnvelope,
  TaskActivationReason,
  WorkHorizon,
} from "./types.js";

const WINDOW_CONSUMING_ACTIONS = new Set<ControlActionKind>([
  "approve_start",
  "modify_candidate",
  "cancel_candidate",
  "keep_current_task",
  "replace_active",
  "approve_request",
  "reject_request",
]);

function timestamp(): string {
  return Date.now().toString();
}

function id(prefix: string): string {
  return `${prefix}-${crypto.randomUUID()}`;
}

function toManagedTaskClass(value: string | undefined): ManagedTaskClass | null {
  switch (value) {
    case "complex":
      return "COMPLEX";
    case "huge":
      return "HUGE";
    case "max":
      return "MAX";
    default:
      return null;
  }
}

function normalizeConfidence(value: number | null | undefined): number | null {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return null;
  }
  if (value >= 0 && value <= 1) {
    return Math.max(0, Math.min(100, Math.round(value * 100)));
  }
  return Math.max(0, Math.min(100, Math.round(value)));
}

function normalizeRequirementOrigin(
  value: string | undefined,
  fallback: "initial_decision" | "task_change" | "user_clarification",
): "initial_decision" | "task_change" | "user_clarification" {
  switch (value) {
    case "initial_decision":
    case "task_change":
    case "user_clarification":
      return value;
    default:
      return fallback;
  }
}

function findDecision<K extends HostSemanticBundle["decisions"][number]["decision_kind"]>(
  bundle: HostSemanticBundle,
  kind: K,
) {
  return bundle.decisions.find((decision) => decision.decision_kind === kind);
}

function buildChatFallbackSemanticDecision(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): SemanticDecisionEnvelope {
  return {
    decision_id: id("decision"),
    host_session_id: hostSessionId,
    host_message_ref: hostMessageRef ?? bundle.input_ref ?? null,
    managed_task_ref: null,
    interaction_lane: "chat",
    managed_task_class: null,
    work_horizon: null,
    task_activation_reason: null,
    title: null,
    summary: null,
    expected_outcome: null,
    requirement_items: [],
    workspace_ref: null,
    repo_ref: null,
    allowed_roots: [],
    secret_classes: [],
    confidence: null,
    created_at: bundle.issued_at || timestamp(),
  };
}

export function normalizeHostSemanticBundle(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): { semanticDecision: SemanticDecisionEnvelope | null; controlAction: ControlAction | null } {
  if (bundle.schema_version.major !== 0) {
    throw new Error(`unsupported schema major version: ${bundle.schema_version.major}`);
  }

  const interactionLaneDecision = findDecision(bundle, "interaction_lane");
  const controlActionDecision = findDecision(bundle, "control_action");
  if (!interactionLaneDecision || interactionLaneDecision.decision_kind !== "interaction_lane") {
    if (controlActionDecision && controlActionDecision.decision_kind === "control_action") {
      return {
        semanticDecision: null,
        controlAction: mapControlActionPayload(controlActionDecision.payload),
      };
    }
    return {
      semanticDecision: buildChatFallbackSemanticDecision(bundle, hostSessionId, hostMessageRef),
      controlAction: null,
    };
  }
  const interactionLane = interactionLaneDecision.payload.interaction_lane;
  const activationDecision = findDecision(bundle, "task_activation_reason");
  const classDecision = findDecision(bundle, "managed_task_class");
  const horizonDecision = findDecision(bundle, "work_horizon");
  const taskChangeDecision = findDecision(bundle, "task_change");

  const managedTaskClass =
    classDecision && classDecision.decision_kind === "managed_task_class"
      ? toManagedTaskClass(classDecision.payload.managed_task_class)
      : null;
  const workHorizon =
    horizonDecision && horizonDecision.decision_kind === "work_horizon"
      ? horizonDecision.payload.work_horizon
      : null;
  const taskActivationReason =
    activationDecision && activationDecision.decision_kind === "task_activation_reason"
      ? activationDecision.payload.task_activation_reason
      : null;

  if (
    interactionLane === "managed_task_candidate" &&
    (!managedTaskClass || !workHorizon || !taskActivationReason)
  ) {
    throw new Error(
      "managed_task_candidate requires managed_task_class, work_horizon and task_activation_reason",
    );
  }
  const managedTaskRef = interactionLaneDecision.payload.managed_task_ref ?? null;
  if (interactionLane === "managed_task_active" && !managedTaskRef) {
    throw new Error("managed_task_active requires managed_task_ref");
  }

  const semanticDecision: SemanticDecisionEnvelope = {
    decision_id: id("decision"),
    host_session_id: hostSessionId,
    host_message_ref: hostMessageRef ?? bundle.input_ref ?? null,
    managed_task_ref: managedTaskRef,
    interaction_lane: interactionLane,
    managed_task_class: managedTaskClass,
    work_horizon: workHorizon,
    task_activation_reason: taskActivationReason,
    title: interactionLaneDecision.payload.title ?? null,
    summary: interactionLaneDecision.payload.summary ?? null,
    expected_outcome: interactionLaneDecision.payload.expected_outcome ?? null,
    requirement_items: (interactionLaneDecision.payload.requirement_items ?? []).map((item) => ({
      text: item.text,
      origin: normalizeRequirementOrigin(item.origin, "initial_decision"),
    })),
    workspace_ref: interactionLaneDecision.payload.workspace_ref ?? null,
    repo_ref: interactionLaneDecision.payload.repo_ref ?? null,
    allowed_roots: interactionLaneDecision.payload.allowed_roots ?? [],
    secret_classes: interactionLaneDecision.payload.secret_classes ?? [],
    confidence: normalizeConfidence(interactionLaneDecision.confidence),
    created_at: bundle.issued_at || timestamp(),
  };

  if (
    controlActionDecision &&
    controlActionDecision.decision_kind === "control_action"
  ) {
    return {
      semanticDecision,
      controlAction: mapControlActionPayload(controlActionDecision.payload),
    };
  }

  if (taskChangeDecision && taskChangeDecision.decision_kind === "task_change") {
    return {
      semanticDecision,
      controlAction: {
        action_id: id("action"),
        managed_task_ref: managedTaskRef,
        kind: "request_task_change",
        actor: "user",
        payload: {
          title: null,
          summary: taskChangeDecision.payload.summary ?? null,
          expected_outcome: taskChangeDecision.payload.expected_outcome ?? null,
          requirement_items: (taskChangeDecision.payload.requirement_items ?? []).map((item) => ({
            text: item.text,
            origin: normalizeRequirementOrigin(item.origin, "task_change"),
          })),
          allowed_roots: taskChangeDecision.payload.allowed_roots ?? [],
          secret_classes: taskChangeDecision.payload.secret_classes ?? [],
          workspace_ref: taskChangeDecision.payload.workspace_ref ?? null,
          repo_ref: taskChangeDecision.payload.repo_ref ?? null,
          rationale: taskChangeDecision.payload.rationale ?? null,
        },
        source_decision_ref: semanticDecision.decision_id,
        decision_token: null,
      },
    };
  }

  return {
    semanticDecision,
    controlAction: null,
  };
}

function mapControlActionPayload(
  payload: Extract<
    HostSemanticBundle["decisions"][number],
    { decision_kind: "control_action" }
  >["payload"],
): ControlAction {
  if (WINDOW_CONSUMING_ACTIONS.has(payload.action_kind) && !payload.decision_token) {
    throw new Error(`${payload.action_kind} requires decision_token to fail closed`);
  }

  return {
    action_id: id("action"),
    managed_task_ref: payload.managed_task_ref ?? null,
    kind: payload.action_kind,
    actor: "user",
    payload: {
      title: payload.payload?.title ?? null,
      summary: payload.payload?.summary ?? null,
      expected_outcome: payload.payload?.expected_outcome ?? null,
      requirement_items: (payload.payload?.requirement_items ?? []).map((item) => ({
        text: item.text,
        origin: normalizeRequirementOrigin(item.origin, "task_change"),
      })),
      allowed_roots: payload.payload?.allowed_roots ?? [],
      secret_classes: payload.payload?.secret_classes ?? [],
      workspace_ref: payload.payload?.workspace_ref ?? null,
      repo_ref: payload.payload?.repo_ref ?? null,
      rationale: payload.payload?.rationale ?? null,
    },
    source_decision_ref: payload.source_decision_ref ?? null,
    decision_token: payload.decision_token ?? null,
  };
}

export function mapHostSemanticBundleToSemanticDecision(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): SemanticDecisionEnvelope | null {
  return normalizeHostSemanticBundle(bundle, hostSessionId, hostMessageRef).semanticDecision;
}

export function mapHostSemanticBundleToControlAction(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): ControlAction | null {
  return normalizeHostSemanticBundle(bundle, hostSessionId, hostMessageRef).controlAction;
}

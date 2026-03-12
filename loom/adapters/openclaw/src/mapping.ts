import type {
  BoundaryRecommendation,
  ChangeExecutionSurface,
  ControlAction,
  ControlActionEnvelope,
  ControlActionKind,
  HostSemanticBundle,
  HostSemanticDecision,
  InteractionLane,
  ManagedTaskClass,
  SemanticDecisionBatchEnvelope,
  SemanticDecisionEnvelope,
  TaskActivationReason,
  TaskChangeClassification,
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
const DEDUPE_WINDOW = "PT10M";

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

function normalizeConfidence(value: number | null | undefined): number {
  if (typeof value !== "number" || Number.isNaN(value)) {
    throw new Error("semantic decision confidence is required");
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

type NormalizedTaskChangeDecision = {
  classification: TaskChangeClassification;
  executionSurface: ChangeExecutionSurface;
  boundaryRecommendation: BoundaryRecommendation;
};

function isTaskChangeClassification(value: unknown): value is TaskChangeClassification {
  switch (value) {
    case "same_task_minor":
    case "same_task_material":
    case "same_task_structural":
    case "boundary_conflict_candidate":
      return true;
    default:
      return false;
  }
}

function isChangeExecutionSurface(value: unknown): value is ChangeExecutionSurface {
  switch (value) {
    case "future_only":
    case "active_stage":
    case "completed_scope":
      return true;
    default:
      return false;
  }
}

function isBoundaryRecommendation(value: unknown): value is BoundaryRecommendation {
  switch (value) {
    case "absorb_change":
    case "require_confirmation":
    case "open_boundary_confirmation":
      return true;
    default:
      return false;
  }
}

function normalizeTaskChangeDecision(
  decision: Extract<HostSemanticDecision, { decision_kind: "task_change" }> | undefined,
): NormalizedTaskChangeDecision | null {
  if (!decision) {
    return null;
  }

  const { classification, execution_surface, boundary_recommendation } = decision.payload;
  if (
    !isTaskChangeClassification(classification) ||
    !isChangeExecutionSurface(execution_surface) ||
    !isBoundaryRecommendation(boundary_recommendation)
  ) {
    throw new Error(
      "task_change requires classification, execution_surface and boundary_recommendation",
    );
  }

  return {
    classification,
    executionSurface: execution_surface,
    boundaryRecommendation: boundary_recommendation,
  };
}

function findDecision<K extends HostSemanticDecision["decision_kind"]>(
  bundle: HostSemanticBundle,
  kind: K,
) {
  return bundle.decisions.find((decision): decision is Extract<HostSemanticDecision, { decision_kind: K }> => {
    return decision.decision_kind === kind;
  });
}

function validateDecisionRefs(bundle: HostSemanticBundle): void {
  const seen = new Set<string>();
  for (const decision of bundle.decisions) {
    if (typeof decision.decision_ref !== "string" || decision.decision_ref.trim().length === 0) {
      throw new Error("host semantic decision requires decision_ref");
    }
    if (seen.has(decision.decision_ref)) {
      throw new Error(`duplicate decision_ref in host semantic bundle: ${decision.decision_ref}`);
    }
    seen.add(decision.decision_ref);
  }
}

function validateSemanticDecisionCardinality(bundle: HostSemanticBundle): void {
  const semanticKinds = new Set<string>();
  let controlActionCount = 0;
  for (const decision of bundle.decisions) {
    if (decision.decision_kind === "control_action") {
      controlActionCount += 1;
      continue;
    }
    if (semanticKinds.has(decision.decision_kind)) {
      throw new Error(`duplicate semantic decision kind in host semantic bundle: ${decision.decision_kind}`);
    }
    semanticKinds.add(decision.decision_kind);
  }
  if (controlActionCount > 1) {
    throw new Error("host semantic bundle supports at most one control_action decision");
  }
}

function buildChatFallbackSemanticDecision(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): SemanticDecisionEnvelope {
  return {
    decision_ref: id("decision"),
    host_session_id: hostSessionId,
    host_message_ref: hostMessageRef ?? bundle.input_ref ?? null,
    managed_task_ref: null,
    decision_kind: "interaction_lane",
    decision_source: "adapter_fallback",
    rationale: "fallback to chat",
    confidence: 20,
    source_model_ref: bundle.source_model_ref,
    issued_at: bundle.issued_at || timestamp(),
    decision_payload: {
      interaction_lane: "chat",
    },
  };
}

function mapInteractionLaneDecision(
  decision: Extract<HostSemanticDecision, { decision_kind: "interaction_lane" }>,
  hostSessionId: string,
  hostMessageRef: string | undefined,
  sourceModelRef: string,
  issuedAt: string,
): SemanticDecisionEnvelope {
  return {
    decision_ref: decision.decision_ref,
    host_session_id: hostSessionId,
    host_message_ref: hostMessageRef ?? null,
    managed_task_ref: decision.payload.managed_task_ref ?? null,
    decision_kind: "interaction_lane",
    decision_source: decision.decision_source,
    rationale: decision.rationale,
    confidence: normalizeConfidence(decision.confidence),
    source_model_ref: sourceModelRef,
    issued_at: issuedAt,
    decision_payload: {
      interaction_lane: decision.payload.interaction_lane,
      managed_task_ref: decision.payload.managed_task_ref,
      title: decision.payload.title,
      summary: decision.payload.summary,
      expected_outcome: decision.payload.expected_outcome,
      requirement_items: (decision.payload.requirement_items ?? []).map((item) => ({
        text: item.text,
        origin: normalizeRequirementOrigin(item.origin, "initial_decision"),
      })),
      workspace_ref: decision.payload.workspace_ref,
      repo_ref: decision.payload.repo_ref,
      allowed_roots: decision.payload.allowed_roots ?? [],
      secret_classes: decision.payload.secret_classes ?? [],
    },
  };
}

function mapSemanticDecision(
  decision: Exclude<HostSemanticDecision, { decision_kind: "control_action" }>,
  hostSessionId: string,
  hostMessageRef: string | undefined,
  managedTaskRef: string | null,
  sourceModelRef: string,
  issuedAt: string,
): SemanticDecisionEnvelope {
  switch (decision.decision_kind) {
    case "interaction_lane":
      return mapInteractionLaneDecision(decision, hostSessionId, hostMessageRef, sourceModelRef, issuedAt);
    case "task_activation_reason":
      return {
        decision_ref: decision.decision_ref,
        host_session_id: hostSessionId,
        host_message_ref: hostMessageRef ?? null,
        managed_task_ref: managedTaskRef,
        decision_kind: "task_activation_reason",
        decision_source: decision.decision_source,
        rationale: decision.rationale,
        confidence: normalizeConfidence(decision.confidence),
        source_model_ref: sourceModelRef,
        issued_at: issuedAt,
        decision_payload: {
          task_activation_reason: decision.payload.task_activation_reason,
        },
      };
    case "managed_task_class": {
      const managedTaskClass = toManagedTaskClass(decision.payload.managed_task_class);
      if (!managedTaskClass) {
        throw new Error(`unsupported managed_task_class: ${decision.payload.managed_task_class}`);
      }
      return {
        decision_ref: decision.decision_ref,
        host_session_id: hostSessionId,
        host_message_ref: hostMessageRef ?? null,
        managed_task_ref: managedTaskRef,
        decision_kind: "managed_task_class",
        decision_source: decision.decision_source,
        rationale: decision.rationale,
        confidence: normalizeConfidence(decision.confidence),
        source_model_ref: sourceModelRef,
        issued_at: issuedAt,
        decision_payload: {
          managed_task_class: managedTaskClass,
        },
      };
    }
    case "work_horizon":
      return {
        decision_ref: decision.decision_ref,
        host_session_id: hostSessionId,
        host_message_ref: hostMessageRef ?? null,
        managed_task_ref: managedTaskRef,
        decision_kind: "work_horizon",
        decision_source: decision.decision_source,
        rationale: decision.rationale,
        confidence: normalizeConfidence(decision.confidence),
        source_model_ref: sourceModelRef,
        issued_at: issuedAt,
        decision_payload: {
          work_horizon: decision.payload.work_horizon,
        },
      };
    case "task_change": {
      const taskChange = normalizeTaskChangeDecision(decision);
      if (!taskChange) {
        throw new Error("task_change decision normalization failed");
      }
      return {
        decision_ref: decision.decision_ref,
        host_session_id: hostSessionId,
        host_message_ref: hostMessageRef ?? null,
        managed_task_ref: managedTaskRef,
        decision_kind: "task_change",
        decision_source: decision.decision_source,
        rationale: decision.rationale,
        confidence: normalizeConfidence(decision.confidence),
        source_model_ref: sourceModelRef,
        issued_at: issuedAt,
        decision_payload: {
          classification: taskChange.classification,
          execution_surface: taskChange.executionSurface,
          boundary_recommendation: taskChange.boundaryRecommendation,
        },
      };
    }
  }
}

function mapControlActionPayload(
  payload: Extract<
    HostSemanticDecision,
    { decision_kind: "control_action" }
  >["payload"],
  fallbackManagedTaskRef: string | null,
): ControlAction {
  if (WINDOW_CONSUMING_ACTIONS.has(payload.action_kind) && !payload.decision_token) {
    throw new Error(`${payload.action_kind} requires decision_token to fail closed`);
  }
  if (payload.action_kind === "request_task_change" && !payload.source_decision_ref) {
    throw new Error("request_task_change requires source_decision_ref");
  }

  return {
    action_id: id("action"),
    managed_task_ref: payload.managed_task_ref ?? fallbackManagedTaskRef ?? null,
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

function mapControlActionEnvelope(
  decision: Extract<HostSemanticDecision, { decision_kind: "control_action" }>,
  fallbackManagedTaskRef: string | null,
  sourceModelRef: string,
  issuedAt: string,
): ControlActionEnvelope {
  return {
    decision_ref: decision.decision_ref,
    decision_source: decision.decision_source,
    rationale: decision.rationale,
    confidence: normalizeConfidence(decision.confidence),
    source_model_ref: sourceModelRef,
    issued_at: issuedAt,
    action: mapControlActionPayload(decision.payload, fallbackManagedTaskRef),
  };
}

function validateBundleDecisions(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): {
  interactionLaneDecision: Extract<HostSemanticDecision, { decision_kind: "interaction_lane" }> | null;
  controlActionDecision: Extract<HostSemanticDecision, { decision_kind: "control_action" }> | null;
  managedTaskRef: string | null;
} {
  if (bundle.schema_version.major !== 0) {
    throw new Error(`unsupported schema major version: ${bundle.schema_version.major}`);
  }
  validateDecisionRefs(bundle);
  validateSemanticDecisionCardinality(bundle);

  const interactionLaneDecision = findDecision(bundle, "interaction_lane");
  const controlActionDecision = findDecision(bundle, "control_action");
  if (!interactionLaneDecision) {
    return {
      interactionLaneDecision: null,
      controlActionDecision: controlActionDecision ?? null,
      managedTaskRef: null,
    };
  }

  const interactionLane = interactionLaneDecision.payload.interaction_lane;
  const activationDecision = findDecision(bundle, "task_activation_reason");
  const classDecision = findDecision(bundle, "managed_task_class");
  const horizonDecision = findDecision(bundle, "work_horizon");
  const taskChangeDecision = findDecision(bundle, "task_change");
  const managedTaskRef = interactionLaneDecision.payload.managed_task_ref ?? null;

  if (
    interactionLane === "managed_task_candidate" &&
    (!classDecision || !horizonDecision || !activationDecision)
  ) {
    throw new Error(
      "managed_task_candidate requires managed_task_class, work_horizon and task_activation_reason",
    );
  }
  if (interactionLane === "managed_task_active" && !managedTaskRef) {
    throw new Error("managed_task_active requires managed_task_ref");
  }
  if (controlActionDecision?.payload.action_kind === "request_task_change") {
    if (interactionLane !== "managed_task_active") {
      throw new Error("request_task_change requires interaction_lane=managed_task_active");
    }
    if (!taskChangeDecision) {
      throw new Error(
        "request_task_change requires a paired task_change judgment with classification, execution_surface and boundary_recommendation",
      );
    }
    if (!controlActionDecision.payload.source_decision_ref) {
      throw new Error("request_task_change requires source_decision_ref");
    }
    if (controlActionDecision.payload.source_decision_ref !== taskChangeDecision.decision_ref) {
      throw new Error("request_task_change source_decision_ref must reference the paired task_change decision_ref");
    }
    if (
      controlActionDecision.payload.managed_task_ref &&
      managedTaskRef &&
      controlActionDecision.payload.managed_task_ref !== managedTaskRef
    ) {
      throw new Error("request_task_change managed_task_ref must match the active interaction_lane task");
    }
  }
  if (
    controlActionDecision &&
    controlActionDecision.payload.managed_task_ref &&
    managedTaskRef &&
    controlActionDecision.payload.managed_task_ref !== managedTaskRef &&
    controlActionDecision.payload.action_kind !== "approve_start" &&
    controlActionDecision.payload.action_kind !== "modify_candidate" &&
    controlActionDecision.payload.action_kind !== "cancel_candidate"
  ) {
    throw new Error("control_action managed_task_ref must match the semantic decision task context");
  }

  return {
    interactionLaneDecision,
    controlActionDecision: controlActionDecision ?? null,
    managedTaskRef,
  };
}

export function normalizeHostSemanticBundle(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): { semanticDecisions: SemanticDecisionEnvelope[]; controlAction: ControlActionEnvelope | null } {
  const issuedAt = bundle.issued_at || timestamp();
  const { interactionLaneDecision, controlActionDecision, managedTaskRef } =
    validateBundleDecisions(bundle, hostSessionId, hostMessageRef);

  if (!interactionLaneDecision) {
    if (controlActionDecision) {
      return {
        semanticDecisions: [],
        controlAction: mapControlActionEnvelope(
          controlActionDecision,
          managedTaskRef,
          bundle.source_model_ref,
          issuedAt,
        ),
      };
    }
    return {
      semanticDecisions: [buildChatFallbackSemanticDecision(bundle, hostSessionId, hostMessageRef)],
      controlAction: null,
    };
  }

  const semanticDecisions = bundle.decisions
    .filter(
      (
        decision,
      ): decision is Exclude<HostSemanticDecision, { decision_kind: "control_action" }> =>
        decision.decision_kind !== "control_action",
    )
    .map((decision) =>
      mapSemanticDecision(
        decision,
        hostSessionId,
        hostMessageRef,
        managedTaskRef,
        bundle.source_model_ref,
        issuedAt,
      ),
    );

  return {
    semanticDecisions,
    controlAction: controlActionDecision
      ? mapControlActionEnvelope(
          controlActionDecision,
          managedTaskRef,
          bundle.source_model_ref,
          issuedAt,
        )
      : null,
  };
}

export function mapHostSemanticBundleToSemanticDecisions(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): SemanticDecisionEnvelope[] {
  return normalizeHostSemanticBundle(bundle, hostSessionId, hostMessageRef).semanticDecisions;
}

export function mapHostSemanticBundleToControlActionEnvelope(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): ControlActionEnvelope | null {
  return normalizeHostSemanticBundle(bundle, hostSessionId, hostMessageRef).controlAction;
}

export function mapHostSemanticBundleToIngressBatch(
  bundle: HostSemanticBundle,
  hostSessionId: string,
  hostMessageRef?: string,
): SemanticDecisionBatchEnvelope {
  const canonicalHostMessageRef = hostMessageRef ?? bundle.input_ref ?? null;
  const normalized = normalizeHostSemanticBundle(bundle, hostSessionId, canonicalHostMessageRef ?? undefined);
  return {
    meta: {
      ingress_id: id("ingress"),
      received_at: timestamp(),
      causation_id: null,
      correlation_id: id("corr"),
      dedupe_window: DEDUPE_WINDOW,
    },
    host_session_id: hostSessionId,
    host_message_ref: canonicalHostMessageRef,
    input_ref: canonicalHostMessageRef ?? bundle.input_ref,
    source_model_ref: bundle.source_model_ref,
    issued_at: bundle.issued_at || timestamp(),
    rationale_summary: bundle.rationale_summary ?? null,
    semantic_decisions: normalized.semanticDecisions,
    control_action: normalized.controlAction,
  };
}

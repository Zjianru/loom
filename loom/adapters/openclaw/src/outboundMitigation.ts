import type { KernelOutboundPayload, OutboundDelivery } from "./types.js";

export type DeliveryVisibilityClass =
  | "interactive_primary"
  | "interactive_secondary"
  | "async_notice";

export type InjectFailureClass =
  | "host_not_ready"
  | "bridge_or_transport_failure"
  | "hard_failure";

export type InteractiveDeliveryState = {
  deliveryId: string;
  hostSessionId: string;
  visibilityClass: DeliveryVisibilityClass;
  firstAttemptAt: string;
  lastAttemptAt: string;
  hostNotReadyCount: number;
  enteredQuiescentAt?: string | null;
  lastFailureClass?: InjectFailureClass | null;
};

export type StartCardHostNotReadyPlan = {
  delayMs: number;
  enterQuiescent: boolean;
  armLocalTimer: boolean;
  logLateDeliveryRisk: boolean;
};

export const INITIAL_START_CARD_GRACE_MS = 500;
export const START_CARD_FAST_RETRY_DELAYS_MS = [1_000, 2_000, 4_000] as const;
export const QUIESCENT_PARK_MS = 24 * 60 * 60 * 1_000;

const FAILURE_PREFIX_PATTERN =
  /^(host_not_ready|bridge_or_transport_failure|hard_failure):\s*/;
const INJECT_FAILURE_PREFIX = /^gateway chat\.inject failed with exit \d+:\s*/;

function messageText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function normalizeFailureMessage(message: string): string {
  return message
    .replace(FAILURE_PREFIX_PATTERN, "")
    .replace(INJECT_FAILURE_PREFIX, "")
    .trim();
}

export function classifyDeliveryVisibility(
  payload: KernelOutboundPayload,
): DeliveryVisibilityClass {
  switch (payload.type) {
    case "start_card":
      return "interactive_primary";
    case "boundary_card":
    case "approval_request":
      return "interactive_secondary";
    case "result_summary":
    case "suppress_host_message":
    case "tool_decision":
      return "async_notice";
  }
}

export function classifyInjectFailure(error: unknown): InjectFailureClass {
  const normalized = normalizeFailureMessage(messageText(error)).toLowerCase();
  if (normalized.includes("transcript file not found")) {
    return "host_not_ready";
  }
  if (
    normalized.includes("timed out") ||
    normalized.includes("timeout") ||
    normalized.includes("econnrefused") ||
    normalized.includes("connection refused") ||
    normalized.includes("fetch failed") ||
    normalized.includes("bridge unavailable")
  ) {
    return "bridge_or_transport_failure";
  }
  if (
    normalized.includes("session not found") ||
    normalized.includes("invalid payload") ||
    normalized.includes("returned invalid payload") ||
    normalized.includes("non-json stdout")
  ) {
    return "hard_failure";
  }
  return "hard_failure";
}

export function formatInjectLastError(
  failureClass: InjectFailureClass,
  error: unknown,
): string {
  return `${failureClass}: ${normalizeFailureMessage(messageText(error))}`;
}

export function shouldApplyInitialStartCardGrace(
  outbound: OutboundDelivery,
  state?: InteractiveDeliveryState | null,
): boolean {
  return (
    outbound.payload.type === "start_card" &&
    outbound.attempts === 1 &&
    !state
  );
}

export function planStartCardHostNotReadyRetry(
  attempts: number,
): StartCardHostNotReadyPlan {
  if (attempts <= 2) {
    return {
      delayMs: START_CARD_FAST_RETRY_DELAYS_MS[0],
      enterQuiescent: false,
      armLocalTimer: true,
      logLateDeliveryRisk: false,
    };
  }
  if (attempts === 3) {
    return {
      delayMs: START_CARD_FAST_RETRY_DELAYS_MS[1],
      enterQuiescent: false,
      armLocalTimer: true,
      logLateDeliveryRisk: false,
    };
  }
  if (attempts === 4) {
    return {
      delayMs: START_CARD_FAST_RETRY_DELAYS_MS[2],
      enterQuiescent: false,
      armLocalTimer: true,
      logLateDeliveryRisk: false,
    };
  }
  return {
    delayMs: QUIESCENT_PARK_MS,
    enterQuiescent: true,
    armLocalTimer: false,
    logLateDeliveryRisk: true,
  };
}

export function shouldWakeQuiescentOnLoomCommand(verb: string): boolean {
  return verb === "help" || verb === "probe";
}

export function isLoomCommandText(text: string): boolean {
  return /^\s*\/loom\b/.test(text);
}

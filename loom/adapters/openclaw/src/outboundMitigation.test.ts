import { describe, expect, it } from "vitest";

import {
  INITIAL_START_CARD_GRACE_MS,
  QUIESCENT_PARK_MS,
  START_CARD_FAST_RETRY_DELAYS_MS,
  classifyDeliveryVisibility,
  classifyInjectFailure,
  formatInjectLastError,
  planStartCardHostNotReadyRetry,
  shouldWakeQuiescentOnLoomCommand,
  shouldApplyInitialStartCardGrace,
} from "./outboundMitigation.js";
import type { KernelOutboundPayload, OutboundDelivery } from "./types.js";

function buildPayload(type: KernelOutboundPayload["type"]): KernelOutboundPayload {
  switch (type) {
    case "start_card":
      return {
        type,
        data: {
          managed_task_ref: "task-1",
          decision_token: "decision-1",
          managed_task_class: "COMPLEX",
          work_horizon: "maintenance",
          task_activation_reason: "explicit_start_task",
          title: "Managed task",
          summary: "Show the managed-task start card",
          expected_outcome: "The user sees the start card first",
          recommended_pack_ref: "coding_pack",
          allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
        },
      };
    case "boundary_card":
      return {
        type,
        data: {
          managed_task_ref: "task-1",
          candidate_managed_task_ref: "task-2",
          decision_token: "decision-1",
          active_task_summary: "Continue current work",
          candidate_task_summary: "Replace with a new task",
          boundary_reason: "scope_change",
          allowed_actions: ["keep_current_task", "replace_active"],
        },
      };
    case "approval_request":
      return {
        type,
        data: {
          managed_task_ref: "task-1",
          decision_token: "decision-1",
          approval_scope: "task_execution",
          allowed_actions: ["approve_request", "reject_request"],
          why_now: "Need approval before executing.",
          risk_summary: "The task touches production code.",
        },
      };
    case "result_summary":
      return {
        type,
        data: {
          managed_task_ref: "task-1",
          outcome: "completed",
          acceptance_verdict: "accepted",
          summary: "Work completed successfully.",
          final_scope_version: 1,
          proof_of_work_excerpt: {
            run_summary: "Completed the implementation.",
            evidence_refs: [],
            artifact_manifest_excerpt: [],
            acceptance_basis_excerpt: [],
          },
          next_actions_excerpt: [],
        },
      };
    case "suppress_host_message":
      return {
        type,
        data: {
          reason: "ReplacedByStructuredPayload",
        },
      };
    case "tool_decision":
      return {
        type,
        data: {
          managed_task_ref: "task-1",
          decision_value: "allow",
          decision_area: "task_execution",
          summary: "Execution is allowed.",
        },
      };
    case "status_notice":
      return {
        type,
        data: {
          managed_task_ref: "task-1",
          notice_kind: "stage_entered",
          stage_ref: "phase-entry-execute",
          headline: "Entered execute stage",
          summary: "Task entered execute and queued worker dispatch.",
          detail: "Execution authorization is active and worker dispatch has been queued.",
        },
      };
  }
}

function buildOutbound(
  payload: KernelOutboundPayload,
  attempts: number,
): OutboundDelivery {
  return {
    delivery_id: "delivery-1",
    host_session_id: "session-1",
    managed_task_ref: "task-1",
    correlation_id: "corr-1",
    causation_id: null,
    payload,
    delivery_status: attempts === 1 ? "pending" : "retry_scheduled",
    attempts,
    max_attempts: 6,
    next_attempt_at: null,
    expires_at: null,
    last_error: null,
    created_at: "1000",
    acked_at: null,
  };
}

describe("outbound mitigation helpers", () => {
  it("classifies payload visibility by delivery type", () => {
    expect(classifyDeliveryVisibility(buildPayload("start_card"))).toBe("interactive_primary");
    expect(classifyDeliveryVisibility(buildPayload("boundary_card"))).toBe("interactive_secondary");
    expect(classifyDeliveryVisibility(buildPayload("approval_request"))).toBe("interactive_secondary");
    expect(classifyDeliveryVisibility(buildPayload("result_summary"))).toBe("async_notice");
    expect(classifyDeliveryVisibility(buildPayload("status_notice"))).toBe("async_notice");
    expect(classifyDeliveryVisibility(buildPayload("tool_decision"))).toBe("async_notice");
  });

  it("classifies chat.inject failures into host readiness, transport, and hard failures", () => {
    expect(
      classifyInjectFailure(
        new Error("gateway chat.inject failed with exit 1: failed to write transcript: transcript file not found"),
      ),
    ).toBe("host_not_ready");
    expect(
      classifyInjectFailure(new Error("gateway chat.inject failed with exit 1: session not found")),
    ).toBe("hard_failure");
    expect(
      classifyInjectFailure(new Error("gateway timeout while invoking chat.inject")),
    ).toBe("bridge_or_transport_failure");
    expect(
      classifyInjectFailure(new Error("gateway chat.inject returned invalid payload: not-json")),
    ).toBe("hard_failure");
  });

  it("prefixes last_error values with the classified failure kind", () => {
    expect(
      formatInjectLastError(
        "host_not_ready",
        new Error("gateway chat.inject failed with exit 1: failed to write transcript: transcript file not found"),
      ),
    ).toBe("host_not_ready: failed to write transcript: transcript file not found");
  });

  it("applies initial grace only to the first start-card delivery attempt", () => {
    expect(shouldApplyInitialStartCardGrace(buildOutbound(buildPayload("start_card"), 1))).toBe(true);
    expect(shouldApplyInitialStartCardGrace(buildOutbound(buildPayload("start_card"), 2))).toBe(false);
    expect(shouldApplyInitialStartCardGrace(buildOutbound(buildPayload("result_summary"), 1))).toBe(false);
  });

  it("maps start-card host_not_ready retries into fast retries and then quiescent park", () => {
    expect(INITIAL_START_CARD_GRACE_MS).toBe(500);
    expect(START_CARD_FAST_RETRY_DELAYS_MS).toEqual([1_000, 2_000, 4_000]);
    expect(planStartCardHostNotReadyRetry(2)).toEqual({
      delayMs: 1_000,
      enterQuiescent: false,
      armLocalTimer: true,
      logLateDeliveryRisk: false,
    });
    expect(planStartCardHostNotReadyRetry(3)).toEqual({
      delayMs: 2_000,
      enterQuiescent: false,
      armLocalTimer: true,
      logLateDeliveryRisk: false,
    });
    expect(planStartCardHostNotReadyRetry(4)).toEqual({
      delayMs: 4_000,
      enterQuiescent: false,
      armLocalTimer: true,
      logLateDeliveryRisk: false,
    });
    expect(planStartCardHostNotReadyRetry(5)).toEqual({
      delayMs: QUIESCENT_PARK_MS,
      enterQuiescent: true,
      armLocalTimer: false,
      logLateDeliveryRisk: true,
    });
  });

  it("only wakes quiescent deliveries on /loom help or /loom probe", () => {
    expect(shouldWakeQuiescentOnLoomCommand("help")).toBe(true);
    expect(shouldWakeQuiescentOnLoomCommand("probe")).toBe(true);
    expect(shouldWakeQuiescentOnLoomCommand("approve")).toBe(false);
  });
});

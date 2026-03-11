import { describe, expect, it } from "vitest";

import {
  mapHostSemanticBundleToControlAction,
  mapHostSemanticBundleToSemanticDecision,
} from "./mapping.js";
import type { HostSemanticBundle } from "./types.js";

function bundle(decisions: HostSemanticBundle["decisions"]): HostSemanticBundle {
  return {
    schema_version: { major: 0, minor: 1 },
    input_ref: "host-message-1",
    source_model_ref: "host-model",
    issued_at: "1000",
    decisions,
  };
}

describe("mapping", () => {
  it("falls back to chat when interaction_lane is missing", () => {
    const envelope = mapHostSemanticBundleToSemanticDecision(
      bundle([
        {
          decision_kind: "managed_task_class",
          decision_source: "host_model",
          confidence: 96,
          rationale: "The user likely wants a managed task.",
          payload: {
            managed_task_class: "complex",
          },
        },
      ]),
      "session-chat-fallback",
      "host-message-1",
    );

    expect(envelope).toBeTruthy();
    if (!envelope) {
      throw new Error("expected chat fallback semantic decision");
    }
    expect(envelope.interaction_lane).toBe("chat");
    expect(envelope.managed_task_ref).toBeNull();
    expect(envelope.managed_task_class).toBeNull();
    expect(mapHostSemanticBundleToControlAction(bundle([]), "session-chat-fallback")).toBeNull();
  });

  it("maps a pure control_action bundle without synthesizing a semantic decision", () => {
    const semanticDecision = mapHostSemanticBundleToSemanticDecision(
      bundle([
        {
          decision_kind: "control_action",
          decision_source: "user_control_action",
          confidence: 99,
          rationale: "The user explicitly approved the candidate from the control surface.",
          payload: {
            action_kind: "approve_start",
            managed_task_ref: "task-1",
            decision_token: "start-win-1",
          },
        },
      ]),
      "session-control-only",
      "host-message-1",
    );

    const controlAction = mapHostSemanticBundleToControlAction(
      bundle([
        {
          decision_kind: "control_action",
          decision_source: "user_control_action",
          confidence: 99,
          rationale: "The user explicitly approved the candidate from the control surface.",
          payload: {
            action_kind: "approve_start",
            managed_task_ref: "task-1",
            decision_token: "start-win-1",
          },
        },
      ]),
      "session-control-only",
      "host-message-1",
    );

    expect(semanticDecision).toBeNull();
    expect(controlAction).toMatchObject({
      kind: "approve_start",
      managed_task_ref: "task-1",
      decision_token: "start-win-1",
    });
  });

  it("maps host semantic bundle to a chat semantic decision", () => {
    const envelope = mapHostSemanticBundleToSemanticDecision(
      bundle([
        {
          decision_kind: "interaction_lane",
          decision_source: "host_model",
          confidence: 0.9,
          rationale: "This is a plain chat turn.",
          payload: {
            interaction_lane: "chat",
            summary: "Explain the current Loom state",
          },
        },
      ]),
      "session-chat",
      "host-message-1",
    );

    expect(envelope).toBeTruthy();
    if (!envelope) {
      throw new Error("expected chat semantic decision");
    }
    expect(envelope.interaction_lane).toBe("chat");
    expect(envelope.managed_task_class).toBeNull();
    expect(envelope.confidence).toBe(90);
  });

  it("normalizes fractional confidence scores into kernel wire values", () => {
    const envelope = mapHostSemanticBundleToSemanticDecision(
      bundle([
        {
          decision_kind: "interaction_lane",
          decision_source: "host_model",
          confidence: 0.42,
          rationale: "This is a low-confidence chat fallback.",
          payload: {
            interaction_lane: "chat",
          },
        },
      ]),
      "session-confidence",
      "host-message-1",
    );

    expect(envelope).toBeTruthy();
    if (!envelope) {
      throw new Error("expected confidence normalization semantic decision");
    }
    expect(envelope.confidence).toBe(42);
  });

  it("fails closed when managed task candidate misses class, horizon or activation reason", () => {
    expect(() =>
      mapHostSemanticBundleToSemanticDecision(
        bundle([
          {
            decision_kind: "interaction_lane",
            decision_source: "host_model",
            confidence: 96,
            rationale: "This should become a managed task.",
            payload: {
              interaction_lane: "managed_task_candidate",
              summary: "Start the task",
            },
          },
        ]),
        "session-managed",
      ),
    ).toThrow(/managed_task_candidate/);
  });

  it("requires decision_token for window-consuming control actions", () => {
    expect(() =>
      mapHostSemanticBundleToControlAction(
        bundle([
          {
            decision_kind: "interaction_lane",
            decision_source: "host_model",
            confidence: 98,
            rationale: "This input is about the active task.",
            payload: {
              interaction_lane: "managed_task_active",
              managed_task_ref: "task-1",
            },
          },
          {
            decision_kind: "control_action",
            decision_source: "user_control_action",
            confidence: 99,
            rationale: "The user explicitly approved the candidate.",
            payload: {
              action_kind: "approve_start",
              managed_task_ref: "task-1",
            },
          },
        ]),
        "session-control",
      ),
    ).toThrow(/decision_token/);
  });

  it("normalizes unknown requirement origins into kernel-safe enum values", () => {
    const envelope = mapHostSemanticBundleToSemanticDecision(
      bundle([
        {
          decision_kind: "interaction_lane",
          decision_source: "host_model",
          confidence: 98,
          rationale: "The user explicitly asked to start a managed task.",
          payload: {
            interaction_lane: "managed_task_candidate",
            summary: "Start the host bridge validation task",
            requirement_items: [{ text: "Check bridge state", origin: "用户请求" }],
          },
        },
        {
          decision_kind: "task_activation_reason",
          decision_source: "host_model",
          confidence: 98,
          rationale: "This is an explicit managed-task request.",
          payload: {
            task_activation_reason: "explicit_user_request",
          },
        },
        {
          decision_kind: "managed_task_class",
          decision_source: "host_model",
          confidence: 98,
          rationale: "The task is bounded and concrete.",
          payload: {
            managed_task_class: "complex",
          },
        },
        {
          decision_kind: "work_horizon",
          decision_source: "host_model",
          confidence: 98,
          rationale: "This is maintenance work on the existing bridge wiring.",
          payload: {
            work_horizon: "maintenance",
          },
        },
      ]),
      "session-origin-normalize",
      "host-message-1",
    );

    expect(envelope).toBeTruthy();
    if (!envelope) {
      throw new Error("expected managed-task semantic decision");
    }
    expect(envelope.requirement_items).toEqual([
      {
        text: "Check bridge state",
        origin: "initial_decision",
      },
    ]);
  });
});

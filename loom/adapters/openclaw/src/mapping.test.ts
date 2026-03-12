import { describe, expect, it } from "vitest";

import {
  mapHostSemanticBundleToControlActionEnvelope,
  mapHostSemanticBundleToIngressBatch,
  mapHostSemanticBundleToSemanticDecisions,
} from "./mapping.js";
import type { HostSemanticBundle, HostSemanticDecision } from "./types.js";

function withDecisionRefs(
  decisions: Array<Record<string, unknown>>,
): HostSemanticDecision[] {
  return decisions.map((decision, index) => ({
    decision_ref: `decision-${index + 1}`,
    ...decision,
  })) as HostSemanticDecision[];
}

function bundle(decisions: HostSemanticBundle["decisions"]): HostSemanticBundle {
  return {
    schema_version: { major: 0, minor: 1 },
    input_ref: "host-message-1",
    source_model_ref: "host-model",
    issued_at: "1000",
    decisions,
  };
}

function uncheckedBundle(decisions: unknown[]): HostSemanticBundle {
  return bundle(decisions as HostSemanticBundle["decisions"]);
}

describe("mapping", () => {
  it("falls back to chat when interaction_lane is missing", () => {
    const semanticDecisions = mapHostSemanticBundleToSemanticDecisions(
      bundle(
        withDecisionRefs([
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
      ),
      "session-chat-fallback",
      "host-message-1",
    );

    expect(semanticDecisions).toHaveLength(1);
    expect(semanticDecisions[0]).toMatchObject({
      decision_kind: "interaction_lane",
      decision_source: "adapter_fallback",
      managed_task_ref: null,
      decision_payload: {
        interaction_lane: "chat",
      },
    });
    expect(
      mapHostSemanticBundleToControlActionEnvelope(bundle([]), "session-chat-fallback"),
    ).toBeNull();
  });

  it("maps a pure control_action bundle without synthesizing a semantic decision", () => {
    const semanticDecisions = mapHostSemanticBundleToSemanticDecisions(
      bundle(
        withDecisionRefs([
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
      ),
      "session-control-only",
      "host-message-1",
    );

    const controlAction = mapHostSemanticBundleToControlActionEnvelope(
      bundle(
        withDecisionRefs([
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
      ),
      "session-control-only",
      "host-message-1",
    );

    expect(semanticDecisions).toEqual([]);
    expect(controlAction).toMatchObject({
      decision_ref: "decision-1",
      action: {
        kind: "approve_start",
        managed_task_ref: "task-1",
        decision_token: "start-win-1",
      },
    });
  });

  it("maps host semantic bundle to a chat semantic decision", () => {
    const semanticDecisions = mapHostSemanticBundleToSemanticDecisions(
      bundle(
        withDecisionRefs([
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
      ),
      "session-chat",
      "host-message-1",
    );

    expect(semanticDecisions).toHaveLength(1);
    expect(semanticDecisions[0]).toMatchObject({
      decision_ref: "decision-1",
      decision_kind: "interaction_lane",
      managed_task_ref: null,
      confidence: 90,
      decision_payload: {
        interaction_lane: "chat",
        summary: "Explain the current Loom state",
      },
    });
  });

  it("normalizes fractional confidence scores into kernel wire values", () => {
    const semanticDecisions = mapHostSemanticBundleToSemanticDecisions(
      bundle(
        withDecisionRefs([
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
      ),
      "session-confidence",
      "host-message-1",
    );

    expect(semanticDecisions[0]?.confidence).toBe(42);
  });

  it("fails closed when managed task candidate misses class, horizon or activation reason", () => {
    expect(() =>
      mapHostSemanticBundleToSemanticDecisions(
        bundle(
          withDecisionRefs([
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
        ),
        "session-managed",
      ),
    ).toThrow(/managed_task_candidate/);
  });

  it("requires decision_token for window-consuming control actions", () => {
    expect(() =>
      mapHostSemanticBundleToControlActionEnvelope(
        bundle(
          withDecisionRefs([
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
        ),
        "session-control",
      ),
    ).toThrow(/decision_token/);
  });

  it("maps an explicit request_task_change only when the governance judgment is present", () => {
    const taskChangeDecisionRef = "decision-task-change";
    const sourceBundle = uncheckedBundle(
      withDecisionRefs([
        {
          decision_ref: "decision-interaction",
          decision_kind: "interaction_lane",
          decision_source: "host_model",
          confidence: 0.95,
          rationale: "This input changes the current active task.",
          payload: {
            interaction_lane: "managed_task_active",
            managed_task_ref: "task-active-explicit",
          },
        },
        {
          decision_ref: taskChangeDecisionRef,
          decision_kind: "task_change",
          decision_source: "host_model",
          confidence: 0.88,
          rationale: "This is a future-only minor change to the current task.",
          payload: {
            classification: "same_task_minor",
            execution_surface: "future_only",
            boundary_recommendation: "absorb_change",
          },
        },
        {
          decision_ref: "decision-control",
          decision_kind: "control_action",
          decision_source: "user_control_action",
          confidence: 0.99,
          rationale: "The host explicitly requested a task change.",
          payload: {
            action_kind: "request_task_change",
            managed_task_ref: "task-active-explicit",
            source_decision_ref: taskChangeDecisionRef,
            payload: {
              summary: "Expand the task to include notification retries.",
              expected_outcome: "The task scope includes retry delivery behavior.",
            },
          },
        },
      ]),
    );

    const semanticDecisions = mapHostSemanticBundleToSemanticDecisions(
      sourceBundle,
      "session-task-change-explicit",
      "host-message-1",
    );
    const controlAction = mapHostSemanticBundleToControlActionEnvelope(
      sourceBundle,
      "session-task-change-explicit",
      "host-message-1",
    );
    const ingressBatch = mapHostSemanticBundleToIngressBatch(
      sourceBundle,
      "session-task-change-explicit",
      "host-message-1",
    );

    expect(semanticDecisions).toHaveLength(2);
    expect(semanticDecisions[0]).toMatchObject({
      decision_kind: "interaction_lane",
      managed_task_ref: "task-active-explicit",
      decision_payload: {
        interaction_lane: "managed_task_active",
      },
    });
    expect(semanticDecisions[1]).toMatchObject({
      decision_ref: taskChangeDecisionRef,
      decision_kind: "task_change",
      managed_task_ref: "task-active-explicit",
      decision_payload: {
        classification: "same_task_minor",
        execution_surface: "future_only",
        boundary_recommendation: "absorb_change",
      },
    });
    expect(controlAction).toMatchObject({
      decision_ref: "decision-control",
      action: {
        kind: "request_task_change",
        managed_task_ref: "task-active-explicit",
        source_decision_ref: taskChangeDecisionRef,
        payload: {
          summary: "Expand the task to include notification retries.",
          expected_outcome: "The task scope includes retry delivery behavior.",
        },
      },
    });
    expect(ingressBatch).toMatchObject({
      host_session_id: "session-task-change-explicit",
      semantic_decisions: [
        { decision_kind: "interaction_lane" },
        { decision_kind: "task_change", decision_ref: taskChangeDecisionRef },
      ],
      control_action: {
        action: {
          kind: "request_task_change",
          source_decision_ref: taskChangeDecisionRef,
        },
      },
    });
  });

  it("does not synthesize request_task_change from a task_change judgment alone", () => {
    const controlAction = mapHostSemanticBundleToControlActionEnvelope(
      bundle(
        withDecisionRefs([
          {
            decision_kind: "interaction_lane",
            decision_source: "host_model",
            confidence: 0.94,
            rationale: "This input changes the active managed task.",
            payload: {
              interaction_lane: "managed_task_active",
              managed_task_ref: "task-active-1",
            },
          },
          {
            decision_kind: "task_change",
            decision_source: "host_model",
            confidence: 0.87,
            rationale: "Only future work changes, so this can be absorbed if explicitly requested.",
            payload: {
              classification: "same_task_minor",
              execution_surface: "future_only",
              boundary_recommendation: "absorb_change",
            },
          },
        ]),
      ),
      "session-task-change-implicit",
      "host-message-1",
    );

    expect(controlAction).toBeNull();
  });

  it("fails closed when request_task_change lacks a paired task_change judgment", () => {
    expect(() =>
      mapHostSemanticBundleToControlActionEnvelope(
        bundle(
          withDecisionRefs([
            {
              decision_kind: "interaction_lane",
              decision_source: "host_model",
              confidence: 0.95,
              rationale: "This input targets the current active task.",
              payload: {
                interaction_lane: "managed_task_active",
                managed_task_ref: "task-active-2",
              },
            },
            {
              decision_kind: "control_action",
              decision_source: "user_control_action",
              confidence: 0.99,
              rationale: "The host explicitly classified this as a task change request.",
              payload: {
                action_kind: "request_task_change",
                managed_task_ref: "task-active-2",
                source_decision_ref: "decision-missing",
                payload: {
                  summary: "Expand the task scope to include notification retries.",
                },
              },
            },
          ]),
        ),
        "session-task-change-missing-judgment",
        "host-message-1",
      ),
    ).toThrow(/task_change/i);
  });

  it("fails closed when task_change judgment misses governance fields", () => {
    expect(() =>
      mapHostSemanticBundleToControlActionEnvelope(
        uncheckedBundle(
          withDecisionRefs([
            {
              decision_kind: "interaction_lane",
              decision_source: "host_model",
              confidence: 0.94,
              rationale: "This input changes the active managed task.",
              payload: {
                interaction_lane: "managed_task_active",
                managed_task_ref: "task-active-3",
              },
            },
            {
              decision_kind: "task_change",
              decision_source: "host_model",
              confidence: 0.74,
              rationale: "The host identified a task change but did not complete the governance judgment.",
              payload: {
                summary: "Also update the notification transport contract.",
              },
            },
            {
              decision_kind: "control_action",
              decision_source: "user_control_action",
              confidence: 0.99,
              rationale: "The host explicitly requested the task change action.",
              payload: {
                action_kind: "request_task_change",
                managed_task_ref: "task-active-3",
                source_decision_ref: "decision-2",
                payload: {
                  summary: "Also update the notification transport contract.",
                },
              },
            },
          ]),
        ),
        "session-task-change-missing-governance",
        "host-message-1",
      ),
    ).toThrow(/classification|execution_surface|boundary_recommendation/);
  });

  it("normalizes unknown requirement origins into kernel-safe enum values", () => {
    const semanticDecisions = mapHostSemanticBundleToSemanticDecisions(
      bundle(
        withDecisionRefs([
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
              task_activation_reason: "explicit_start_task",
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
      ),
      "session-origin-normalize",
      "host-message-1",
    );

    expect(semanticDecisions[0]).toMatchObject({
      decision_kind: "interaction_lane",
      decision_payload: {
        requirement_items: [
          {
            text: "Check bridge state",
            origin: "initial_decision",
          },
        ],
      },
    });
  });
});

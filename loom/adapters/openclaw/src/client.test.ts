import { beforeEach, describe, expect, it, vi } from "vitest";

import { createLoomBridgeClient, normalizeKernelOutboundPayload } from "./client.js";

describe("loom bridge client", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("surfaces bridge response bodies for authenticated POST failures", async () => {
    globalThis.fetch = vi.fn(async () => new Response("signature mismatch", { status: 401 })) as never;

    const client = createLoomBridgeClient("http://127.0.0.1:6417", {
      adapterId: "loom-openclaw",
      getCredential: () => ({
        bridge_instance_id: "bridge-1",
        adapter_id: "loom-openclaw",
        secret_ref: "secret-ref-1",
        rotation_epoch: 1,
        session_secret: "session-secret-1",
      }),
    });

    await expect(
      client.postCapabilitySnapshot({
        capability_snapshot_ref: "cap-1",
        host_session_id: "agent:main:main",
        allowed_tools: ["loom_emit_host_semantic_bundle"],
        readable_roots: ["/Users/codez/.openclaw"],
        writable_roots: ["/Users/codez/.openclaw"],
        secret_classes: ["repo"],
        max_budget_band: "standard",
        available_agent_ids: ["main", "coder", "product_analyst"],
        supports_spawn_agents: true,
        supports_pause: true,
        supports_resume: true,
        supports_interrupt: true,
        recorded_at: "1000",
      }),
    ).rejects.toThrow("bridge POST /v1/ingress/capability-snapshot failed with 401: signature mismatch");
  });

  it("normalizes snake_case outbound variants emitted by the Rust bridge", () => {
    expect(
      normalizeKernelOutboundPayload({
        start_card: {
          managed_task_ref: "task-1",
          decision_token: "decision-1",
          managed_task_class: "COMPLEX",
          work_horizon: "maintenance",
          task_activation_reason: "explicit_user_request",
          title: "Managed task",
          summary: "Check the host bridge",
          expected_outcome: "Verify end-to-end host wiring",
          allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
        },
      }),
    ).toMatchObject({
      type: "start_card",
      data: {
        managed_task_ref: "task-1",
        decision_token: "decision-1",
      },
    });
  });

  it("supports host execution polling and ack endpoints", async () => {
    globalThis.fetch = vi
      .fn(async (input: RequestInfo | URL) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.includes("/v1/host-execution/next?host_session_id=agent%3Amain%3Amain")) {
          return new Response(
            JSON.stringify({
            command_id: "exec-1",
            managed_task_ref: "task-1",
            run_ref: "run-1",
            binding_id: "binding-1",
            role_kind: "worker",
            host_session_id: "agent:main:main",
            host_agent_id: "coder",
            prompt: "Implement the task",
            label: "loom-worker-task-1",
            status: "pending",
            child_session_key: "agent:coder:child-legacy",
            child_run_id: "run-child-legacy",
            output_summary: null,
            artifact_refs: [],
            issued_at: "1000",
            acked_at: null,
            completed_at: null,
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/host-execution/exec-1/ack")) {
          return new Response(null, { status: 200 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      }) as never;

    const client = createLoomBridgeClient("http://127.0.0.1:6417", {
      adapterId: "loom-openclaw",
      getCredential: () => ({
        bridge_instance_id: "bridge-1",
        adapter_id: "loom-openclaw",
        secret_ref: "secret-ref-1",
        rotation_epoch: 1,
        session_secret: "session-secret-1",
      }),
    });

    await expect(client.nextHostExecution("agent:main:main")).resolves.toMatchObject({
      command_id: "exec-1",
      role_kind: "worker",
      host_agent_id: "coder",
      host_child_execution_ref: "agent:coder:child-legacy",
      host_child_run_ref: "run-child-legacy",
    });
    await expect(client.ackHostExecution("exec-1")).resolves.toBe(true);
  });
});

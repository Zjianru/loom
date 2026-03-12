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
        host_kind: "openclaw",
        host_session_id: "agent:main:main",
        available_agents: [{ host_agent_ref: "main", display_name: "main", available: true }],
        available_models: [],
        available_tools: [{ tool_name: "loom_emit_host_semantic_bundle", available: true }],
        spawn_capabilities: [
          {
            runtime_kind: "subagent",
            available: true,
            host_agent_scope: {
              mode: "explicit_list",
              allowed_host_agent_refs: ["coder", "product_analyst"],
            },
            supports_resume_session: false,
            supports_thread_spawn: false,
            supports_parent_progress_stream: false,
          },
          {
            runtime_kind: "acp",
            available: false,
            host_agent_scope: {
              mode: "none",
              allowed_host_agent_refs: [],
            },
            supports_resume_session: false,
            supports_thread_spawn: false,
            supports_parent_progress_stream: false,
          },
        ],
        session_scope: {
          session_role: "main",
          control_scope: "children",
          source: "derived",
        },
        allowed_tools: ["loom_emit_host_semantic_bundle"],
        readable_roots: ["/Users/codez/.openclaw"],
        writable_roots: ["/Users/codez/.openclaw"],
        secret_classes: ["repo"],
        max_budget_band: "standard",
        render_capabilities: {
          supports_text_render: true,
          supports_inline_actions: false,
          supports_message_suppression: true,
        },
        background_task_support: true,
        async_notice_support: true,
        available_agent_ids: ["main", "coder", "product_analyst"],
        supports_spawn_agents: true,
        supports_pause: false,
        supports_resume: false,
        supports_interrupt: false,
        worker_control_capabilities: {
          supports_pause: false,
          supports_resume: false,
          supports_cancel: false,
          supports_soft_interrupt: false,
          supports_hard_interrupt: false,
        },
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
          task_activation_reason: "explicit_start_task",
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

  it("normalizes snake_case status_notice outbound variants emitted by the Rust bridge", () => {
    expect(
      normalizeKernelOutboundPayload({
        status_notice: {
          managed_task_ref: "task-1",
          notice_kind: "stage_entered",
          stage_ref: "phase-entry-execute",
          headline: "Entered execute stage",
          summary: "Task entered execute and queued worker dispatch.",
          detail: "Worker dispatch queued through host execution.",
        },
      }),
    ).toMatchObject({
      type: "status_notice",
      data: {
        managed_task_ref: "task-1",
        notice_kind: "stage_entered",
        stage_ref: "phase-entry-execute",
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

  it("reads the current control surface projection from the authenticated bridge query", async () => {
    globalThis.fetch = vi
      .fn(async (input: RequestInfo | URL) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.includes("/v1/control-surface/current?host_session_id=agent%3Amain%3Amain")) {
          return new Response(
            JSON.stringify({
              host_session_id: "agent:main:main",
              surface_type: "start_card",
              managed_task_ref: "task-1",
              decision_token: "decision-1",
              allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
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

    await expect(client.readCurrentControlSurface("agent:main:main")).resolves.toMatchObject({
      host_session_id: "agent:main:main",
      surface_type: "start_card",
      managed_task_ref: "task-1",
      decision_token: "decision-1",
      allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
    });
  });
});

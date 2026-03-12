import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

import plugin from "./index.js";

type MockHookHandler = (event: unknown, ctx: Record<string, unknown>) => unknown | Promise<unknown>;
type MockToolFactory = (ctx: Record<string, unknown>) => unknown;
type MockService = {
  id: string;
  start?: (ctx?: Record<string, unknown>) => unknown | Promise<unknown>;
  stop?: (ctx?: Record<string, unknown>) => unknown | Promise<unknown>;
};
type MockCommand = {
  name: string;
  description: string;
  acceptsArgs?: boolean;
  requireAuth?: boolean;
  handler: (ctx: Record<string, unknown>) => unknown | Promise<unknown>;
};

type MockApiOptions = {
  resolvePath?: (path: string) => string;
};

function createMockApi(
  rootDir: string,
  pluginConfig?: unknown,
  configOverride?: Record<string, unknown>,
  options?: MockApiOptions,
) {
  const hooks = new Map<string, MockHookHandler[]>();
  const toolFactories: Array<{ factory: MockToolFactory; options?: Record<string, unknown> }> = [];
  const services: MockService[] = [];
  const commands: MockCommand[] = [];
  const logs: Array<{ level: string; message: string; meta?: unknown }> = [];
  const enqueueSystemEvent = vi.fn();
  const runCommandWithTimeout = vi.fn(async () => ({
    stdout: JSON.stringify({ ok: true, messageId: "inject-1" }),
    stderr: "",
    code: 0,
    signal: null,
    killed: false,
    termination: "exit" as const,
  }));
  const config = configOverride ?? {
    agents: {
      defaults: {
        workspace: join(rootDir, "workspace"),
      },
      list: [{ id: "main", default: true, workspace: join(rootDir, "workspace") }],
    },
    session: {
      dmScope: "main",
    },
  };
  const normalizedPluginConfig =
    pluginConfig ?? {
      bridge: {
        baseUrl: "http://127.0.0.1:6417",
        runtimeRoot: join(rootDir, "runtime"),
      },
    };
  mkdirSync(join(rootDir, "runtime"), { recursive: true });

  return {
    api: {
      config,
      pluginConfig: normalizedPluginConfig,
      logger: {
        info(message: string, meta?: unknown) {
          logs.push({ level: "info", message, meta });
        },
        warn(message: string, meta?: unknown) {
          logs.push({ level: "warn", message, meta });
        },
        error(message: string, meta?: unknown) {
          logs.push({ level: "error", message, meta });
        },
      },
      runtime: {
        system: {
          enqueueSystemEvent,
          runCommandWithTimeout,
        },
        channel: {
          routing: {
            resolveAgentRoute: vi.fn((params: { peer?: { id?: string } }) => ({
              agentId: "main",
              sessionKey: params.peer?.id === "main" ? "agent:main:main" : `agent:main:${params.peer?.id ?? "main"}`,
            })),
            buildAgentSessionKey: vi.fn((params: { agentId?: string; peer?: { id?: string } }) =>
              `agent:${params.agentId ?? "main"}:${params.peer?.id ?? "main"}`,
            ),
          },
        },
      },
      resolvePath(path: string) {
        return options?.resolvePath ? options.resolvePath(path) : resolve(rootDir, path);
      },
      on(eventName: string, handler: MockHookHandler) {
        hooks.set(eventName, [...(hooks.get(eventName) ?? []), handler]);
      },
      registerTool(factory: MockToolFactory, options?: Record<string, unknown>) {
        toolFactories.push({ factory, options });
      },
      registerService(service: MockService) {
        services.push(service);
      },
      registerCommand(command: MockCommand) {
        commands.push(command);
      },
      getConfig<T>(path: string): T | undefined {
        const value = path.split(".").reduce<unknown>((acc, part) => {
          if (!acc || typeof acc !== "object") return undefined;
          return (acc as Record<string, unknown>)[part];
        }, normalizedPluginConfig);
        return value as T | undefined;
      },
    },
    getHook(name: string) {
      return hooks.get(name)?.[0];
    },
    getToolDescriptor() {
      const descriptor = toolFactories[0]?.factory({ sessionKey: "session-1", runId: "run-1" });
      return Array.isArray(descriptor) ? descriptor[0] : descriptor;
    },
    getService(id: string) {
      return services.find((service) => service.id === id);
    },
    getCommand(name: string) {
      return commands.find((command) => command.name === name);
    },
    enqueueSystemEvent,
    runCommandWithTimeout,
    logs,
  };
}

function writeBootstrapTicket(rootDir: string, bridgeInstanceId = "bridge-1") {
  const ticketPath = join(rootDir, "runtime/loom/bootstrap/openclaw/bootstrap-ticket.json");
  mkdirSync(join(rootDir, "runtime/loom/bootstrap/openclaw"), { recursive: true });
  writeFileSync(
    ticketPath,
    JSON.stringify(
      {
        bridge_instance_id: bridgeInstanceId,
        adapter_id: "loom-openclaw",
        ticket_id: "ticket-1",
        ticket_secret: "ticket-secret-1",
        issued_at: "1000",
        expires_at: "2000",
      },
      null,
      2,
    ),
  );
  return ticketPath;
}

function readCommandProbeProjection(rootDir: string, probeDir?: string) {
  return JSON.parse(
    readFileSync(
      probeDir ?? join(rootDir, "runtime/loom/host-bridges/openclaw/command-probe/latest.json"),
      "utf8",
    ),
  ) as {
    recentEvents: Array<{ kind: string; sequence: number }>;
    lastCommand?: {
      resolvedHostSessionId?: string;
      messageReceivedObserved?: boolean;
      matchingMessageOrder?: string;
      matchingMessageEventSequences?: number[];
      commandContext?: {
        keys?: string[];
        fields?: Record<string, { kind?: string; keyCount?: number; keys?: string[]; redacted?: boolean }>;
      };
      latestTurnAtInvoke?: {
        textMatchesCommand?: boolean;
        hostMessageRef?: string;
      };
      latestControlSurfaceAtInvoke?: {
        surfaceType?: string;
        managedTaskRef?: string;
      };
    };
  };
}

type OutboundVariant =
  | { start_card: Record<string, unknown> }
  | { result_summary: Record<string, unknown> }
  | { status_notice: Record<string, unknown> };

function buildStartCardVariant(overrides?: Record<string, unknown>) {
  return {
    start_card: {
      managed_task_ref: "task-1",
      decision_token: "decision-1",
      managed_task_class: "COMPLEX",
      work_horizon: "maintenance",
      task_activation_reason: "explicit_start_task",
      title: "Managed task",
      summary: "Verify visible transcript injection",
      expected_outcome: "Show the start card in host chat",
      recommended_pack_ref: "coding_pack",
      allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
      ...(overrides ?? {}),
    },
  };
}

function buildResultSummaryVariant(overrides?: Record<string, unknown>) {
  return {
    result_summary: {
      managed_task_ref: "task-1",
      outcome: "completed",
      acceptance_verdict: "accepted",
      summary: "Summarize the completed task.",
      final_scope_version: 1,
      proof_of_work_excerpt: {
        run_summary: "Completed the work.",
        evidence_refs: [],
        artifact_manifest_excerpt: [],
        acceptance_basis_excerpt: [],
      },
      next_actions_excerpt: [],
      ...(overrides ?? {}),
    },
  };
}

function buildStatusNoticeVariant(overrides?: Record<string, unknown>) {
  return {
    status_notice: {
      managed_task_ref: "task-1",
      notice_kind: "stage_entered",
      stage_ref: "phase-entry-execute",
      headline: "Entered execute stage",
      summary: "Task entered execute and queued worker dispatch.",
      detail: "Execution authorization is active and worker dispatch has been queued.",
      ...(overrides ?? {}),
    },
  };
}

function buildOutboundDeliveryResponse(options?: {
  deliveryId?: string;
  hostSessionId?: string;
  managedTaskRef?: string;
  correlationId?: string;
  attempts?: number;
  maxAttempts?: number;
  deliveryStatus?: string;
  nextAttemptAt?: string | null;
  lastError?: string | null;
  createdAt?: string;
  ackedAt?: string | null;
  payload?: OutboundVariant;
}) {
  const attempts = options?.attempts ?? 1;
  return {
    delivery_id: options?.deliveryId ?? "delivery-1",
    host_session_id: options?.hostSessionId ?? "session-1",
    managed_task_ref: options?.managedTaskRef ?? "task-1",
    correlation_id: options?.correlationId ?? "corr-1",
    causation_id: null,
    payload: options?.payload ?? buildStartCardVariant(),
    delivery_status: options?.deliveryStatus ?? (attempts === 1 ? "pending" : "retry_scheduled"),
    attempts,
    max_attempts: options?.maxAttempts ?? 6,
    next_attempt_at: options?.nextAttemptAt ?? null,
    expires_at: null,
    last_error: options?.lastError ?? null,
    created_at: options?.createdAt ?? "1002",
    acked_at: options?.ackedAt ?? null,
  };
}

function withDecisionRefs<T extends Record<string, unknown>>(bundle: T): T {
  if (!Array.isArray(bundle.decisions)) {
    return bundle;
  }
  return {
    ...bundle,
    decisions: bundle.decisions.map((decision, index) => {
      if (!decision || typeof decision !== "object") {
        return decision;
      }
      const record = decision as Record<string, unknown>;
      return {
        ...record,
        decision_ref:
          typeof record.decision_ref === "string" && record.decision_ref.trim().length > 0
            ? record.decision_ref
            : `decision-${index + 1}`,
      };
    }),
  };
}

function buildManagedTaskCandidateBundle(overrides?: Record<string, unknown>) {
  return withDecisionRefs({
    schema_version: { major: 0, minor: 1 },
    input_ref: "host-message-1",
    source_model_ref: "host-model",
    issued_at: "1010",
    decisions: [
      {
        decision_kind: "interaction_lane",
        decision_source: "host_model",
        confidence: 0.98,
        rationale: "The user explicitly asked to start a managed task.",
        payload: {
          interaction_lane: "managed_task_candidate",
          summary: "Start the bridge analysis task",
        },
      },
      {
        decision_kind: "task_activation_reason",
        decision_source: "host_model",
        confidence: 0.96,
        rationale: "This is an explicit managed-task request.",
        payload: {
          task_activation_reason: "explicit_start_task",
        },
      },
      {
        decision_kind: "managed_task_class",
        decision_source: "host_model",
        confidence: 0.94,
        rationale: "The task is complex but bounded.",
        payload: {
          managed_task_class: "complex",
        },
      },
      {
        decision_kind: "work_horizon",
        decision_source: "host_model",
        confidence: 0.92,
        rationale: "The task is maintenance work.",
        payload: {
          work_horizon: "maintenance",
        },
      },
    ],
    ...(overrides ?? {}),
  });
}

function hostNotReadyInjectResult() {
  return {
    stdout: "",
    stderr: "failed to write transcript: transcript file not found",
    code: 1,
    signal: null,
    killed: false,
    termination: "exit" as const,
  };
}

function successfulInjectResult(messageId = "inject-1") {
  return {
    stdout: JSON.stringify({ ok: true, messageId }),
    stderr: "",
    code: 0,
    signal: null,
    killed: false,
    termination: "exit" as const,
  };
}

async function ingestManagedTaskTurn(apiKit: ReturnType<typeof createMockApi>, options?: {
  content?: string;
  messageId?: string;
  sessionKey?: string;
}) {
  await apiKit.getHook("message_received")?.(
    {
      content: options?.content ?? "Please start a managed task.",
      metadata: { messageId: options?.messageId ?? "host-message-1" },
    },
    {
      sessionKey: options?.sessionKey ?? "session-1",
      conversationId: options?.sessionKey ?? "session-1",
    },
  );
}

async function submitManagedTaskCandidate(
  apiKit: ReturnType<typeof createMockApi>,
  toolCallId = "tool-call-1",
  overrides?: Record<string, unknown>,
) {
  const descriptor = apiKit.getToolDescriptor() as {
    execute: (toolCallId: string, params: unknown) => Promise<unknown>;
  };
  return descriptor.execute(toolCallId, buildManagedTaskCandidateBundle(overrides));
}

async function executeToolBundle(
  descriptor: { execute: (toolCallId: string, params: unknown) => Promise<unknown> },
  toolCallId: string,
  params: Record<string, unknown>,
) {
  return descriptor.execute(toolCallId, withDecisionRefs(params));
}

describe("loom-openclaw plugin", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("registers synchronously so the real OpenClaw loader does not ignore the plugin", () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);

    const registration = plugin.register(apiKit.api as never);

    expect(registration).not.toBeInstanceOf(Promise);
    expect(registration.runtime.getBridgeStatus()).toBe("disconnected");
  });

  it("registers the runtime hooks, internal tool and peer service", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);

    const registration = await plugin.register(apiKit.api as never);

    expect(apiKit.getHook("message_received")).toBeTypeOf("function");
    expect(apiKit.getHook("before_agent_start")).toBeTypeOf("function");
    expect(apiKit.getHook("before_prompt_build")).toBeTypeOf("function");
    expect(apiKit.getHook("message_sending")).toBeTypeOf("function");
    expect(apiKit.getHook("before_message_write")).toBeTypeOf("function");
    expect(apiKit.getHook("tool_result_persist")).toBeTypeOf("function");
    expect(apiKit.getToolDescriptor()).toMatchObject({ name: "loom_emit_host_semantic_bundle" });
    expect(apiKit.getService("loom-openclaw-peer")).toBeDefined();
    expect(apiKit.getCommand("loom")).toBeDefined();
    expect(registration.runtime.getBridgeStatus()).toBe("disconnected");
  });

  it("service start fails closed when bootstrap material is missing", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    const registration = await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();

    expect(registration.runtime.getBridgeStatus()).toBe("fail_closed");
    expect(apiKit.logs.some((entry) => entry.message.includes("bridge.peer.fail_closed"))).toBe(true);
  });

  it("service start uses configured absolute runtimeRoot in installed mode", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const runtimeRoot = join(rootDir, "runtime");
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(
      rootDir,
      {
        bridge: {
          baseUrl: "http://127.0.0.1:6417",
          runtimeRoot,
        },
      },
      undefined,
      {
        resolvePath(path: string) {
          return resolve("/", path);
        },
      },
    );
    const registration = await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();

    expect(registration.runtime.getBridgeStatus()).toBe("active");
    expect(apiKit.logs.some((entry) => entry.message.includes("bridge.peer.bootstrap_succeeded"))).toBe(true);
  });

  it("fails closed when bridge.runtimeRoot is configured as a relative path", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir, {
      bridge: {
        baseUrl: "http://127.0.0.1:6417",
        runtimeRoot: "runtime",
      },
    });
    const registration = await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();

    expect(registration.runtime.getBridgeStatus()).toBe("fail_closed");
    expect(apiKit.logs.some((entry) => entry.message.includes("bridge.peer.fail_closed"))).toBe(true);
    expect(
      apiKit.logs.some(
        (entry) =>
          entry.message.includes("bridge.peer.fail_closed") &&
          JSON.stringify(entry.meta).includes("bridge.runtimeRoot"),
      ),
    ).toBe(true);
  });

  it("uses host workspace root for current-turn and capability sync even when resolvePath('.') points at /", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const runtimeRoot = join(rootDir, "runtime");
    const workspaceRoot = join(rootDir, "host-workspace");
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(
      rootDir,
      {
        bridge: {
          baseUrl: "http://127.0.0.1:6417",
          runtimeRoot,
        },
      },
      {
        agents: {
          defaults: {
            workspace: workspaceRoot,
          },
          list: [{ id: "main", default: true, workspace: workspaceRoot }],
        },
        session: {
          dmScope: "main",
        },
      },
      {
        resolvePath(path: string) {
          return resolve("/", path);
        },
      },
    );
    await plugin.register(apiKit.api as never);

    let capabilitySnapshot:
      | {
          readable_roots?: string[];
          writable_roots?: string[];
        }
      | undefined;
    let currentTurn:
      | {
          workspace_ref?: string | null;
          repo_ref?: string | null;
        }
      | undefined;

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        capabilitySnapshot = JSON.parse(String(init?.body ?? "{}")) as typeof capabilitySnapshot;
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        currentTurn = JSON.parse(String(init?.body ?? "{}")) as typeof currentTurn;
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });
    await apiKit.getHook("message_received")?.(
      {
        content: "Start the managed task.",
        metadata: { messageId: "host-message-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    expect(capabilitySnapshot).toMatchObject({
      readable_roots: [workspaceRoot],
      writable_roots: [workspaceRoot],
    });
    expect(currentTurn).toMatchObject({
      workspace_ref: workspaceRoot,
      repo_ref: null,
    });
  });

  it("prepends governance instructions after bootstrap succeeds", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    const registration = await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        expect(init?.headers).toBeTruthy();
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    expect(registration.runtime.getBridgeStatus()).toBe("active");
    await apiKit.getHook("message_received")?.(
      {
        content: "Please start a managed task.",
        timestamp: 1000,
        metadata: { messageId: "host-message-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const decision = await apiKit.getHook("before_prompt_build")?.(
      { prompt: "hello" },
      { sessionKey: "session-1", runId: "run-1" },
    );
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("loom_emit_host_semantic_bundle"),
    });
  });

  it("maps message_received conversation identity to the canonical session key used by later hooks", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "Start a managed task from the web chat session.",
        metadata: { messageId: "host-message-1" },
      },
      {
        channelId: "webchat",
        conversationId: "main",
      },
    );

    const decision = await apiKit.getHook("before_prompt_build")?.(
      { prompt: "hello" },
      { sessionKey: "agent:main:main", runId: "run-1" },
    );
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("loom_emit_host_semantic_bundle"),
    });
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("The only safe fallback is interaction_lane=chat."),
    });
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining(
        "interaction_lane=managed_task_candidate plus task_activation_reason, managed_task_class, and work_horizon",
      ),
    });
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("request_task_change"),
    });
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("classification"),
    });
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("execution_surface"),
    });
    expect(decision).toMatchObject({
      prependContext: expect.stringContaining("boundary_recommendation"),
    });
  });

  it("tool schema requires either an interaction_lane or control_action decision shape", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const descriptor = apiKit.getToolDescriptor() as {
      parameters: {
        properties: {
          decisions: {
            anyOf?: Array<{
              contains?: {
                properties?: {
                  decision_kind?: { const?: string };
                };
              };
            }>;
            items?: {
              oneOf?: Array<Record<string, unknown>>;
            };
          };
        };
      };
    };

    expect(
      descriptor.parameters.properties.decisions.anyOf?.map(
        (entry) => entry.contains?.properties?.decision_kind?.const,
      ),
    ).toEqual(expect.arrayContaining(["interaction_lane", "control_action"]));
    expect(descriptor.parameters.properties.decisions.items?.oneOf?.length).toBeGreaterThan(1);
  });

  it("tool schema models task_change as a governance judgment, not a patch payload", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const descriptor = apiKit.getToolDescriptor() as {
      parameters: {
        properties: {
          decisions: {
            items?: {
              oneOf?: Array<{
                properties?: {
                  decision_kind?: { const?: string };
                  payload?: {
                    properties?: Record<string, unknown>;
                    required?: string[];
                  };
                };
              }>;
            };
          };
        };
      };
    };

    const taskChangeSchema = descriptor.parameters.properties.decisions.items?.oneOf?.find(
      (entry) => entry.properties?.decision_kind?.const === "task_change",
    );

    expect(taskChangeSchema).toBeTruthy();
    expect(taskChangeSchema?.properties?.payload?.properties).toMatchObject({
      classification: expect.any(Object),
      execution_surface: expect.any(Object),
      boundary_recommendation: expect.any(Object),
    });
    expect(taskChangeSchema?.properties?.payload?.required).toEqual(
      expect.arrayContaining([
        "classification",
        "execution_surface",
        "boundary_recommendation",
      ]),
    );
    expect(taskChangeSchema?.properties?.payload?.properties).not.toHaveProperty("summary");
    expect(taskChangeSchema?.properties?.payload?.properties).not.toHaveProperty("workspace_ref");
  });

  it("reports command-session resolution and recent slash lifecycle observations through /loom probe", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    await apiKit.getHook("message_received")?.(
      {
        content: "/loom probe",
        metadata: { messageId: "host-message-cmd-1" },
      },
      {
        channelId: "webchat",
        conversationId: "main",
      },
    );

    const command = apiKit.getCommand("loom");
    expect(command).toBeDefined();

    const result = await command?.handler({
      senderId: "user-1",
      channel: "webchat",
      channelId: "webchat",
      isAuthorizedSender: true,
      args: "probe",
      commandBody: "/loom probe",
      config: {},
      from: "user-1",
      to: "main",
    });

    expect(result).toMatchObject({
      text: expect.stringContaining("resolvedHostSessionId: agent:main:main"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("latestTurnTextMatchesCommand: true"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("message_received"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("command_invoked"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("matchingMessageOrder: before_command"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("commandContextKeys:"),
    });

    const projection = readCommandProbeProjection(rootDir);
    expect(projection.recentEvents.map((event) => event.kind)).toEqual([
      "message_received",
      "command_invoked",
    ]);
    expect(projection.lastCommand).toMatchObject({
      resolvedHostSessionId: "agent:main:main",
      messageReceivedObserved: true,
      matchingMessageOrder: "before_command",
      matchingMessageEventSequences: [1],
      latestTurnAtInvoke: {
        textMatchesCommand: true,
        hostMessageRef: "host-message-cmd-1",
      },
    });
    expect(projection.lastCommand?.commandContext?.keys).toEqual(
      expect.arrayContaining(["channel", "channelId", "commandBody", "config", "from", "senderId", "to"]),
    );
    expect(projection.lastCommand?.commandContext?.fields?.config).toMatchObject({
      kind: "object",
      redacted: true,
    });
  });

  it("keeps probe evidence when /loom probe fires before a later message_received observation", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const command = apiKit.getCommand("loom");
    const probeAtInvoke = await command?.handler({
      senderId: "user-1",
      channel: "webchat",
      channelId: "webchat",
      isAuthorizedSender: true,
      args: "probe",
      commandBody: "/loom probe",
      config: {},
      from: "user-1",
      to: "main",
      sessionKey: "agent:main:main",
    });

    expect(probeAtInvoke).toMatchObject({
      text: expect.stringContaining("matchingMessageOrder: not_observed"),
    });

    await apiKit.getHook("message_received")?.(
      {
        content: "/loom probe",
        metadata: { messageId: "host-message-cmd-2" },
      },
      {
        channelId: "webchat",
        conversationId: "main",
      },
    );

    const projection = readCommandProbeProjection(rootDir);
    expect(projection.recentEvents.map((event) => event.kind)).toEqual([
      "command_invoked",
      "message_received",
    ]);
    expect(projection.lastCommand).toMatchObject({
      resolvedHostSessionId: "agent:main:main",
      messageReceivedObserved: true,
      matchingMessageOrder: "after_command",
      matchingMessageEventSequences: [2],
    });
  });

  it("writes command probe projection into the service stateDir when the runtime provides one", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const stateDir = join(rootDir, "plugin-state");
    await apiKit.getService("loom-openclaw-peer")?.start?.({ stateDir });

    await apiKit.getHook("message_received")?.(
      {
        content: "/loom probe",
        metadata: { messageId: "host-message-cmd-3" },
      },
      {
        channelId: "webchat",
        conversationId: "main",
      },
    );

    const command = apiKit.getCommand("loom");
    await command?.handler({
      senderId: "user-1",
      channel: "webchat",
      channelId: "webchat",
      isAuthorizedSender: true,
      args: "probe",
      commandBody: "/loom probe",
      config: {},
      from: "user-1",
      to: "main",
    });

    const projection = readCommandProbeProjection(rootDir, join(stateDir, "command-probe/latest.json"));
    expect(projection.lastCommand).toMatchObject({
      resolvedHostSessionId: "agent:main:main",
      matchingMessageOrder: "before_command",
    });
  });

  it("reads the authoritative current control surface for /loom help and shows executable commands", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
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

    await apiKit.getService("loom-openclaw-peer")?.start?.();

    const command = apiKit.getCommand("loom");
    const result = await command?.handler({
      senderId: "user-1",
      channel: "webchat",
      channelId: "webchat",
      isAuthorizedSender: true,
      commandBody: "/loom",
      config: {},
      from: "user-1",
      to: "main",
    });

    expect(result).toMatchObject({
      text: expect.stringContaining("Current Loom control surface: start_card"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("/loom approve"),
    });
    expect(result).toMatchObject({
      text: expect.stringContaining("/loom cancel"),
    });
  });

  it("submits /loom approve through the control-action normalization path after bridge control-surface lookup", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/control-surface/current?host_session_id=session-1")) {
        return new Response(
          JSON.stringify({
            host_session_id: "session-1",
            surface_type: "start_card",
            managed_task_ref: "task-approve-1",
            decision_token: "decision-approve-1",
            allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=session-1")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=session-1")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "/loom approve",
        metadata: { messageId: "host-message-approve-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const command = apiKit.getCommand("loom");
    const result = await command?.handler({
      senderId: "user-1",
      channel: "webchat",
      channelId: "webchat",
      isAuthorizedSender: true,
      args: "approve",
      commandBody: "/loom approve",
      config: {},
      from: "user-1",
      to: "session-1",
      sessionKey: "session-1",
    });

    expect(result).toMatchObject({
      text: expect.stringContaining("Submitted Loom action: approve_start"),
    });

    const controlActionRequest = fetchMock.mock.calls.find(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/semantic-bundle"),
    );
    expect(controlActionRequest).toBeTruthy();
    const controlActionBody = JSON.parse(String((controlActionRequest?.[1] as RequestInit | undefined)?.body ?? "{}"));
    expect(controlActionBody).toMatchObject({
      control_action: {
        action: {
          kind: "approve_start",
          managed_task_ref: "task-approve-1",
          decision_token: "decision-approve-1",
          actor: "user",
        },
      },
      semantic_decisions: [],
    });
  });

  it("internal tool rejects schema major mismatch and falls back to chat when interaction lane is missing", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    const registration = await plugin.register(apiKit.api as never);

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "Please start a managed task.",
        timestamp: 1000,
        metadata: { messageId: "host-message-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const descriptor = apiKit.getToolDescriptor() as {
      execute: (toolCallId: string, params: unknown) => Promise<unknown>;
    };

    await expect(
      executeToolBundle(descriptor, "tool-call-1", {
        schema_version: { major: 9, minor: 0 },
        input_ref: "host-message-1",
        source_model_ref: "host-model",
        issued_at: "1010",
        decisions: [],
      }),
    ).rejects.toThrow(/schema/i);

    await expect(
      executeToolBundle(descriptor, "tool-call-2", {
        schema_version: { major: 0, minor: 1 },
        input_ref: "host-message-1",
        source_model_ref: "host-model",
        issued_at: "1010",
        decisions: [],
      }),
    ).resolves.toMatchObject({ ok: true });

    const semanticCall = fetchMock.mock.calls.find(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/semantic-bundle"),
    );
    expect(semanticCall).toBeTruthy();
    expect(JSON.parse(String(semanticCall?.[1]?.body))).toMatchObject({
      host_session_id: "session-1",
      semantic_decisions: [
        {
          decision_kind: "interaction_lane",
          managed_task_ref: null,
          decision_payload: {
            interaction_lane: "chat",
          },
        },
      ],
      control_action: null,
    });

    expect(registration.runtime.getBridgeStatus()).toBe("active");
  });

  it("internal tool fails closed when request_task_change lacks a paired task_change judgment", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "Please update the active managed task.",
        timestamp: 1000,
        metadata: { messageId: "host-message-task-change-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const descriptor = apiKit.getToolDescriptor() as {
      execute: (toolCallId: string, params: unknown) => Promise<unknown>;
    };

    await expect(
      executeToolBundle(descriptor, "tool-call-task-change-1", {
        schema_version: { major: 0, minor: 1 },
        input_ref: "host-message-task-change-1",
        source_model_ref: "host-model",
        issued_at: "1010",
        decisions: [
          {
            decision_kind: "interaction_lane",
            decision_source: "host_model",
            confidence: 0.95,
            rationale: "This input targets the current active task.",
            payload: {
              interaction_lane: "managed_task_active",
              managed_task_ref: "task-active-1",
            },
          },
          {
            decision_kind: "control_action",
            decision_source: "user_control_action",
            confidence: 0.99,
            rationale: "The host explicitly requested a task change.",
            payload: {
              action_kind: "request_task_change",
              managed_task_ref: "task-active-1",
              payload: {
                summary: "Expand the task to include notification retries.",
              },
            },
          },
        ],
      }),
    ).rejects.toThrow(/task_change/i);

    expect(fetchMock.mock.calls.some(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/semantic-bundle"),
    )).toBe(false);
  });

  it("internal tool accepts an explicit paired request_task_change bundle", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "Please update the active managed task.",
        timestamp: 1000,
        metadata: { messageId: "host-message-task-change-2" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const descriptor = apiKit.getToolDescriptor() as {
      execute: (toolCallId: string, params: unknown) => Promise<unknown>;
    };

    await expect(
      executeToolBundle(descriptor, "tool-call-task-change-2", {
        schema_version: { major: 0, minor: 1 },
        input_ref: "host-message-task-change-2",
        source_model_ref: "host-model",
        issued_at: "1010",
        decisions: [
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
            decision_kind: "task_change",
            decision_source: "host_model",
            confidence: 0.88,
            rationale: "This is a minor future-only change to the active task.",
            payload: {
              classification: "same_task_minor",
              execution_surface: "future_only",
              boundary_recommendation: "absorb_change",
            },
          },
          {
            decision_kind: "control_action",
            decision_source: "user_control_action",
            confidence: 0.99,
            rationale: "The host explicitly requested a task change.",
            payload: {
              action_kind: "request_task_change",
              managed_task_ref: "task-active-2",
              source_decision_ref: "decision-2",
              payload: {
                summary: "Expand the task to include notification retries.",
                expected_outcome: "The task scope covers retry delivery behavior.",
              },
            },
          },
        ],
      }),
    ).resolves.toMatchObject({
      ok: true,
      controlActionKind: "request_task_change",
    });

    const controlActionRequest = fetchMock.mock.calls.find(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/semantic-bundle"),
    );
    expect(controlActionRequest).toBeTruthy();
    expect(JSON.parse(String((controlActionRequest?.[1] as RequestInit | undefined)?.body ?? "{}"))).toMatchObject({
      semantic_decisions: [
        {
          decision_kind: "interaction_lane",
          managed_task_ref: "task-active-2",
        },
        {
          decision_ref: "decision-2",
          decision_kind: "task_change",
          managed_task_ref: "task-active-2",
          decision_payload: {
            classification: "same_task_minor",
            execution_surface: "future_only",
            boundary_recommendation: "absorb_change",
          },
        },
      ],
      control_action: {
        action: {
          kind: "request_task_change",
          managed_task_ref: "task-active-2",
          source_decision_ref: "decision-2",
          payload: {
            summary: "Expand the task to include notification retries.",
            expected_outcome: "The task scope covers retry delivery behavior.",
          },
        },
      },
    });
  });

  it("posts a pure control action without synthesizing a semantic decision", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "/loom approve",
        metadata: { messageId: "host-message-approve-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const descriptor = apiKit.getToolDescriptor() as {
      execute: (toolCallId: string, params: unknown) => Promise<unknown>;
    };

    await expect(
      executeToolBundle(descriptor, "tool-call-control-1", {
        schema_version: { major: 0, minor: 1 },
        input_ref: "host-message-approve-1",
        source_model_ref: "host-command",
        issued_at: "1010",
        decisions: [
          {
            decision_kind: "control_action",
            decision_source: "user_control_action",
            confidence: 0.99,
            rationale: "The explicit control surface command approves the current start candidate.",
            payload: {
              action_kind: "approve_start",
              managed_task_ref: "task-approve-1",
              decision_token: "start-win-approve-1",
            },
          },
        ],
      }),
    ).resolves.toMatchObject({
      ok: true,
      controlActionKind: "approve_start",
    });

    const controlActionCall = fetchMock.mock.calls.find(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/semantic-bundle"),
    );
    expect(controlActionCall).toBeTruthy();
    const controlActionInit = (controlActionCall as [RequestInfo | URL, RequestInit?] | undefined)?.[1];
    expect(JSON.parse(String(controlActionInit?.body))).toMatchObject({
      control_action: {
        action: {
          kind: "approve_start",
          managed_task_ref: "task-approve-1",
          decision_token: "start-win-approve-1",
        },
      },
      semantic_decisions: [],
    });
  });

  it("canonicalizes bundle input_ref to the latest current turn binding", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "Please start a managed task.",
        metadata: { messageId: "host-message-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const descriptor = apiKit.getToolDescriptor() as {
      execute: (toolCallId: string, params: unknown) => Promise<unknown>;
    };

    await expect(
      executeToolBundle(descriptor, "tool-call-3", {
        schema_version: { major: 0, minor: 1 },
        input_ref: "wrong-ref",
        source_model_ref: "host-model",
        issued_at: "1010",
        decisions: [
          {
            decision_kind: "interaction_lane",
            decision_source: "host_model",
            confidence: 0.98,
            rationale: "The user explicitly asked to start a managed task.",
            payload: {
              interaction_lane: "managed_task_candidate",
              summary: "Start the bridge analysis task",
            },
          },
          {
            decision_kind: "task_activation_reason",
            decision_source: "host_model",
            confidence: 0.96,
            rationale: "This is an explicit managed-task request.",
            payload: {
              task_activation_reason: "explicit_start_task",
            },
          },
          {
            decision_kind: "managed_task_class",
            decision_source: "host_model",
            confidence: 0.94,
            rationale: "The task is complex but bounded.",
            payload: {
              managed_task_class: "complex",
            },
          },
          {
            decision_kind: "work_horizon",
            decision_source: "host_model",
            confidence: 0.92,
            rationale: "The task improves an existing implementation.",
            payload: {
              work_horizon: "improvement",
            },
          },
        ],
      }),
    ).resolves.toMatchObject({ ok: true });

    const semanticCall = fetchMock.mock.calls.find(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/semantic-bundle"),
    );
    const semanticBundleBody = JSON.parse(String((semanticCall?.[1] as RequestInit | undefined)?.body));
    expect(semanticBundleBody.host_message_ref).toBe("host-message-1");
    expect(semanticBundleBody.semantic_decisions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          decision_kind: "interaction_lane",
          host_message_ref: "host-message-1",
          decision_payload: expect.objectContaining({
            interaction_lane: "managed_task_candidate",
          }),
        }),
      ]),
    );
  });

  it("does not resync stable capabilities on every before_agent_start", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    const registration = await plugin.register(apiKit.api as never);

    let capabilityPosts = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        capabilityPosts += 1;
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url} ${String(init?.method ?? "GET")}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-2" });

    expect(capabilityPosts).toBe(1);
    expect(registration.runtime.getBridgeStatus()).toBe("active");
  });

  it("does not downgrade an active peer for non-auth capability sync failures", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    const registration = await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response("temporary capability ingest failure", { status: 500 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });

    expect(registration.runtime.getBridgeStatus()).toBe("active");
    expect(apiKit.logs.some((entry) => entry.message === "bridge.peer.reconnect_requested")).toBe(false);
    expect(
      apiKit.logs.some(
        (entry) => entry.message === "bridge.peer.operation_failed" && entry.level === "warn",
      ),
    ).toBe(true);
  });

  it("keeps governance prompt injection active when auxiliary host-execution polling fails", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    const registration = await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=session-1")) {
        throw new Error("temporary host execution poll failure");
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "开始一个任务：整理文档导航，交给子 agent 持续推进。",
        metadata: { messageId: "host-message-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });

    expect(registration.runtime.getBridgeStatus()).toBe("active");
    expect(apiKit.logs.some((entry) => entry.message === "bridge.peer.reconnect_requested")).toBe(false);
    expect(
      apiKit.logs.some(
        (entry) =>
          entry.message === "bridge.peer.auxiliary_operation_failed" &&
          JSON.stringify(entry.meta).includes("host_execution_poll_failed"),
      ),
    ).toBe(true);

    const promptDecision = await apiKit.getHook("before_prompt_build")?.(
      {},
      { sessionKey: "session-1", runId: "run-1" },
    );
    expect(promptDecision).toMatchObject({
      prependContext: expect.stringContaining("Before any user-visible answer"),
    });
  });

  it("does not request reconnect when child transcript summary fetch fails locally", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    apiKit.runCommandWithTimeout
      .mockResolvedValueOnce({
        stdout: JSON.stringify({ ok: true, runId: "dispatch-run-1" }),
        stderr: "",
        code: 0,
        signal: null,
        killed: false,
        termination: "exit",
      })
      .mockResolvedValueOnce({
        stdout: "",
        stderr: "session missing",
        code: 1,
        signal: null,
        killed: false,
        termination: "exit",
      });

    let hostExecutionPolls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=session-1")) {
        hostExecutionPolls += 1;
        if (hostExecutionPolls === 1) {
          return new Response(
            JSON.stringify({
              command_id: "command-1",
              managed_task_ref: "task-1",
              run_ref: "run-1",
              role_kind: "worker",
              host_session_id: "session-1",
              host_agent_id: "coder",
              label: "Dispatch worker",
              prompt: "Do the work",
              status: "pending",
              created_at: "1000",
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        return new Response(null, { status: 204 });
      }
      if (url.endsWith("/v1/host-execution/command-1/ack")) {
        return new Response(null, { status: 200 });
      }
      if (url.endsWith("/v1/ingress/subagent-lifecycle")) {
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });
    await apiKit.getHook("subagent_spawned")?.(
      {
        childSessionKey: "agent:coder:child-1",
        runId: "child-run-1",
      },
      {
        requesterSessionKey: "agent:main:loom-exec:session-1",
        sessionKey: "agent:main:loom-exec:session-1",
        childSessionKey: "agent:coder:child-1",
      },
    );
    await apiKit.getHook("subagent_ended")?.(
      {
        targetSessionKey: "agent:coder:child-1",
        runId: "child-run-1",
        outcome: "completed",
      },
      {
        requesterSessionKey: "agent:main:loom-exec:session-1",
        sessionKey: "agent:main:loom-exec:session-1",
        childSessionKey: "agent:coder:child-1",
      },
    );

    expect(apiKit.logs.some((entry) => entry.message === "bridge.peer.reconnect_requested")).toBe(false);
    expect(
      apiKit.logs.some(
        (entry) =>
          entry.message === "bridge.peer.auxiliary_operation_failed" &&
          JSON.stringify(entry.meta).includes("subagent_summary_fetch_failed"),
      ),
    ).toBe(true);
  });

  it("dispatches host execution commands and posts subagent lifecycle events back to Loom", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    apiKit.runCommandWithTimeout
      .mockResolvedValueOnce({
        stdout: JSON.stringify({ ok: true, runId: "dispatch-run-1" }),
        stderr: "",
        code: 0,
        signal: null,
        killed: false,
        termination: "exit",
      })
      .mockResolvedValueOnce({
        stdout: JSON.stringify({
          ok: true,
          messages: [
            { role: "assistant", content: [{ type: "text", text: "Summary: worker complete.\nChanged files: loom/README.md\nVerification: cargo test -p loom-harness" }] },
          ],
        }),
        stderr: "",
        code: 0,
        signal: null,
        killed: false,
        termination: "exit",
      });

    let hostExecutionPolls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=session-1")) {
        hostExecutionPolls += 1;
        if (hostExecutionPolls > 1) {
          return new Response(null, { status: 204 });
        }
        return new Response(
          JSON.stringify({
            command_id: "exec-1",
            managed_task_ref: "task-1",
            run_ref: "run-1",
            binding_id: "binding-1",
            role_kind: "worker",
            host_session_id: "session-1",
            host_agent_id: "coder",
            prompt: "Implement the approved task.",
            label: "loom-worker-task-1",
            status: "pending",
            host_child_execution_ref: null,
            host_child_run_ref: null,
            output_summary: null,
            artifact_refs: [],
            issued_at: "1001",
            acked_at: null,
            completed_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/host-execution/exec-1/ack")) {
        return new Response(null, { status: 200 });
      }
      if (url.endsWith("/v1/ingress/subagent-lifecycle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url} ${String(init?.method ?? "GET")}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });

    expect(apiKit.runCommandWithTimeout).toHaveBeenCalledWith(
      expect.arrayContaining(["chat.send"]),
      expect.objectContaining({ timeoutMs: 10_000 }),
    );
    const gatewayCall = apiKit.runCommandWithTimeout.mock.calls[0] as unknown[] | undefined;
    const gatewayArgs = Array.isArray(gatewayCall?.[0]) ? (gatewayCall[0] as string[]) : [];
    expect(gatewayArgs.length).toBeGreaterThan(0);
    expect(gatewayArgs.join(" ")).toContain("loom-exec");
    expect(gatewayArgs.join(" ")).toContain("exec-1");
    const ackCallIndex = fetchMock.mock.calls.findIndex(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/host-execution/exec-1/ack"),
    );
    expect(ackCallIndex).toBeGreaterThan(-1);
    expect(apiKit.runCommandWithTimeout.mock.calls.length).toBeGreaterThan(0);

    await apiKit.getHook("subagent_spawned")?.(
      {
        childSessionKey: "agent:coder:child-1",
        runId: "child-run-1",
        agentId: "coder",
      },
      {
        requesterSessionKey: "agent:main:loom-exec:session-1",
        childSessionKey: "agent:coder:child-1",
      },
    );
    await apiKit.getHook("subagent_ended")?.(
      {
        targetSessionKey: "agent:coder:child-1",
        runId: "child-run-1",
        outcome: { status: "ok" },
      },
      {
        requesterSessionKey: "agent:main:loom-exec:session-1",
        childSessionKey: "agent:coder:child-1",
      },
    );

    const lifecycleCalls = fetchMock.mock.calls.filter(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/subagent-lifecycle"),
    );
    expect(lifecycleCalls).toHaveLength(2);
    const endedPayload = JSON.parse(String((lifecycleCalls[1]?.[1] as RequestInit | undefined)?.body));
    expect(endedPayload).toMatchObject({
      command_id: "exec-1",
      managed_task_ref: "task-1",
      role_kind: "worker",
      event: {
        ended: {
          host_child_execution_ref: "agent:coder:child-1",
          status: "completed",
        },
      },
    });
    expect(endedPayload.event.ended.output_summary).toContain("Verification:");
    expect(endedPayload.event.ended.artifact_refs).toContain("loom/README.md");
  });

  it("does not ack a host execution command until dispatch succeeds and retries on a later poll", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    apiKit.runCommandWithTimeout
      .mockResolvedValueOnce({
        stdout: "",
        stderr: "gateway failed",
        code: 1,
        signal: null,
        killed: false,
        termination: "exit",
      })
      .mockResolvedValueOnce({
        stdout: JSON.stringify({ ok: true, runId: "dispatch-run-1" }),
        stderr: "",
        code: 0,
        signal: null,
        killed: false,
        termination: "exit",
      });

    let hostExecutionPolls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=session-1")) {
        hostExecutionPolls += 1;
        if (hostExecutionPolls > 2) {
          return new Response(null, { status: 204 });
        }
        return new Response(
          JSON.stringify({
            command_id: "exec-1",
            managed_task_ref: "task-1",
            run_ref: "run-1",
            binding_id: "binding-1",
            role_kind: "worker",
            host_session_id: "session-1",
            host_agent_id: "coder",
            prompt: "Implement the approved task.",
            label: "loom-worker-task-1",
            status: "pending",
            host_child_execution_ref: null,
            host_child_run_ref: null,
            output_summary: null,
            artifact_refs: [],
            issued_at: "1001",
            acked_at: null,
            completed_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/host-execution/exec-1/ack")) {
        return new Response(null, { status: 200 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });
    expect(
      fetchMock.mock.calls.some(([input]) =>
        (typeof input === "string" ? input : input.toString()).endsWith("/v1/host-execution/exec-1/ack"),
      ),
    ).toBe(false);

    (apiKit.api.config as { agents?: { list?: Array<Record<string, unknown>> } }).agents = {
      list: [{ id: "main", default: true, subagents: { allowAgents: ["coder"] } }, { id: "coder" }],
    };
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-2" });
    expect(hostExecutionPolls).toBe(3);
    expect(
      fetchMock.mock.calls.filter(([input]) =>
        (typeof input === "string" ? input : input.toString()).endsWith("/v1/host-execution/exec-1/ack"),
      ),
    ).toHaveLength(1);
  });

  it("suppresses current-turn ingestion for spawned child execution sessions", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    apiKit.runCommandWithTimeout
      .mockResolvedValueOnce({
        stdout: JSON.stringify({ ok: true, runId: "dispatch-run-1" }),
        stderr: "",
        code: 0,
        signal: null,
        killed: false,
        termination: "exit",
      })
      .mockResolvedValueOnce({
        stdout: JSON.stringify({
          ok: true,
          messages: [
            {
              role: "assistant",
              content: [{ type: "text", text: "Summary: worker complete.\nChanged files: loom/README.md\nVerification: cargo test -p loom-harness" }],
            },
          ],
        }),
        stderr: "",
        code: 0,
        signal: null,
        killed: false,
        termination: "exit",
      });

    let hostExecutionPolls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=session-1")) {
        hostExecutionPolls += 1;
        if (hostExecutionPolls > 1) {
          return new Response(null, { status: 204 });
        }
        return new Response(
          JSON.stringify({
            command_id: "exec-1",
            managed_task_ref: "task-1",
            run_ref: "run-1",
            binding_id: "binding-1",
            role_kind: "worker",
            host_session_id: "session-1",
            host_agent_id: "coder",
            prompt: "Implement the approved task.",
            label: "loom-worker-task-1",
            status: "pending",
            host_child_execution_ref: null,
            host_child_run_ref: null,
            output_summary: null,
            artifact_refs: [],
            issued_at: "1001",
            acked_at: null,
            completed_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/host-execution/exec-1/ack")) {
        return new Response(null, { status: 200 });
      }
      if (url.endsWith("/v1/ingress/subagent-lifecycle")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "session-1", runId: "run-1" });
    await apiKit.getHook("subagent_spawned")?.(
      {
        childSessionKey: "agent:coder:child-1",
        runId: "child-run-1",
        agentId: "coder",
      },
      {
        requesterSessionKey: "agent:main:loom-exec:session-1",
        childSessionKey: "agent:coder:child-1",
      },
    );

    const currentTurnCallsBefore = fetchMock.mock.calls.filter(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/current-turn"),
    ).length;
    await apiKit.getHook("message_received")?.(
      {
        content: "worker internal reply",
        metadata: { messageId: "child-message-1" },
      },
      { sessionKey: "agent:coder:child-1", conversationId: "agent:coder:child-1" },
    );
    const currentTurnCallsAfter = fetchMock.mock.calls.filter(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/ingress/current-turn"),
    ).length;
    expect(currentTurnCallsAfter).toBe(currentTurnCallsBefore);
  });

  it("syncs only spawnable child agents into the capability snapshot", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const config = {
      agents: {
        list: [
          {
            id: "main",
            default: true,
            subagents: { allowAgents: ["coder", "product_analyst", "ghost"] },
          },
          { id: "coder", tools: { allow: ["sessions_spawn"], deny: [] } },
          { id: "product_analyst", tools: { allow: ["sessions_spawn"], deny: [] } },
          { id: "reviewer", tools: { allow: ["sessions_spawn"], deny: [] } },
        ],
      },
      tools: {
        agentToAgent: { enabled: true },
      },
      session: {
        dmScope: "main",
      },
    };
    const apiKit = createMockApi(rootDir, { bridge: { baseUrl: "http://127.0.0.1:6417" } }, config);
    await plugin.register(apiKit.api as never);

    let capabilitySnapshotBody: string | null = null;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        capabilitySnapshotBody = String(init?.body);
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "agent:main:main", runId: "run-1" });
    expect(
      fetchMock.mock.calls.some(([input]) => {
        const url = typeof input === "string" ? input : input.toString();
        return url.endsWith("/v1/ingress/capability-snapshot");
      }),
    ).toBe(true);
    const snapshot = JSON.parse(capabilitySnapshotBody ?? "null");
    expect(snapshot.host_kind).toBe("openclaw");
    expect(snapshot.available_agent_ids).toEqual(["coder", "product_analyst"]);
    expect(snapshot.supports_spawn_agents).toBe(true);
    expect(snapshot.available_agents).toEqual([
      { host_agent_ref: "main", display_name: "main", available: true },
      { host_agent_ref: "coder", display_name: "coder", available: true },
      { host_agent_ref: "product_analyst", display_name: "product_analyst", available: true },
      { host_agent_ref: "reviewer", display_name: "reviewer", available: true },
    ]);
    expect(snapshot.spawn_capabilities).toEqual([
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
    ]);
    expect(snapshot.session_scope).toEqual({
      session_role: "main",
      control_scope: "children",
      source: "derived",
    });
    expect(snapshot.render_capabilities).toEqual({
      supports_text_render: true,
      supports_inline_actions: false,
      supports_message_suppression: true,
    });
    expect(snapshot.background_task_support).toBe(true);
    expect(snapshot.async_notice_support).toBe(true);
    expect(snapshot.worker_control_capabilities).toEqual({
      supports_pause: false,
      supports_resume: false,
      supports_cancel: false,
      supports_soft_interrupt: false,
      supports_hard_interrupt: false,
    });
  });

  it("marks spawn support false when the current agent explicitly denies sessions_spawn", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const config = {
      agents: {
        defaults: {
          subagents: {
            maxSpawnDepth: 1,
          },
        },
        list: [
          {
            id: "main",
            default: true,
            subagents: { allowAgents: ["coder", "product_analyst"] },
            tools: { deny: ["sessions_spawn"] },
          },
          { id: "coder" },
          { id: "product_analyst" },
        ],
      },
      tools: {
        agentToAgent: { enabled: true },
      },
      session: {
        dmScope: "main",
      },
    };
    const apiKit = createMockApi(rootDir, { bridge: { baseUrl: "http://127.0.0.1:6417" } }, config);
    await plugin.register(apiKit.api as never);

    let capabilitySnapshotBody: string | null = null;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        capabilitySnapshotBody = String(init?.body);
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.(
      {},
      { sessionKey: "agent:main:subagent:child", runId: "run-1" },
    );
    expect(
      fetchMock.mock.calls.some(([input]) => {
        const url = typeof input === "string" ? input : input.toString();
        return url.endsWith("/v1/ingress/capability-snapshot");
      }),
    ).toBe(true);
    const snapshot = JSON.parse(capabilitySnapshotBody ?? "null");
    expect(snapshot.available_agent_ids).toEqual([]);
    expect(snapshot.supports_spawn_agents).toBe(false);
    expect(snapshot.spawn_capabilities).toEqual([
      {
        runtime_kind: "subagent",
        available: false,
        host_agent_scope: {
          mode: "none",
          allowed_host_agent_refs: [],
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
    ]);
    expect(snapshot.session_scope).toEqual({
      session_role: "leaf",
      control_scope: "none",
      source: "derived",
    });
    expect(snapshot.worker_control_capabilities).toEqual({
      supports_pause: false,
      supports_resume: false,
      supports_cancel: false,
      supports_soft_interrupt: false,
      supports_hard_interrupt: false,
    });
  });

  it("advertises ACP runtime capability when ACP dispatch is configured", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const config = {
      agents: {
        list: [{ id: "main", default: true }],
      },
      acp: {
        backend: "acpx",
        defaultAgent: "codex",
      },
      session: {
        dmScope: "main",
      },
    };
    const apiKit = createMockApi(rootDir, { bridge: { baseUrl: "http://127.0.0.1:6417" } }, config);
    await plugin.register(apiKit.api as never);

    let capabilitySnapshotBody: string | null = null;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        capabilitySnapshotBody = String(init?.body);
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("before_agent_start")?.({}, { sessionKey: "agent:main:main", runId: "run-1" });
    expect(
      fetchMock.mock.calls.some(([input]) => {
        const url = typeof input === "string" ? input : input.toString();
        return url.endsWith("/v1/ingress/capability-snapshot");
      }),
    ).toBe(true);
    const snapshot = JSON.parse(capabilitySnapshotBody ?? "null");
    expect(snapshot.spawn_capabilities).toEqual([
      {
        runtime_kind: "subagent",
        available: true,
        host_agent_scope: {
          mode: "none",
          allowed_host_agent_refs: [],
        },
        supports_resume_session: false,
        supports_thread_spawn: false,
        supports_parent_progress_stream: false,
      },
      {
        runtime_kind: "acp",
        available: true,
        host_agent_scope: {
          mode: "all",
          allowed_host_agent_refs: [],
        },
        supports_resume_session: true,
        supports_thread_spawn: true,
        supports_parent_progress_stream: true,
      },
    ]);
    expect(snapshot.session_scope).toEqual({
      session_role: "main",
      control_scope: "children",
      source: "derived",
    });
  });

  it("applies the start-card grace before visible inject and only acks after a successful chat.inject", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-03-11T00:00:00.000Z"));
    try {
      const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
      const runtimeRoot = join(rootDir, "runtime");
      const workspaceRoot = join(rootDir, "host-workspace");
      writeBootstrapTicket(rootDir, "bridge-1");
      const apiKit = createMockApi(
        rootDir,
        {
          bridge: {
            baseUrl: "http://127.0.0.1:6417",
            runtimeRoot,
          },
        },
        {
          agents: {
            defaults: {
              workspace: workspaceRoot,
            },
            list: [{ id: "main", default: true, workspace: workspaceRoot }],
          },
          session: {
            dmScope: "main",
          },
        },
        {
          resolvePath(path: string) {
            return resolve("/", path);
          },
        },
      );
      await plugin.register(apiKit.api as never);

      let outboundPolls = 0;
      const retryRequests: Array<{ scheduledAt: number; body: { next_attempt_at: string; last_error: string } }> = [];
      const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.endsWith("/v1/health")) {
          return new Response(
            JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/bootstrap")) {
          return new Response(
            JSON.stringify({
              bridge_instance_id: "bridge-1",
              credential_id: "cred-1",
              secret_ref: "secret-ref-1",
              rotation_epoch: 1,
              session_secret: "session-secret-1",
              issued_at: "1001",
              expires_at: null,
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/ingress/current-turn")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/capability-snapshot")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/semantic-bundle")) {
          return new Response(null, { status: 202 });
        }
        if (url.includes("/v1/outbound/next?host_session_id=")) {
          outboundPolls += 1;
          const delivery =
            outboundPolls === 1
              ? buildOutboundDeliveryResponse({
                  attempts: 1,
                  payload: buildStartCardVariant(),
                })
              : outboundPolls === 2
                ? buildOutboundDeliveryResponse({
                    attempts: 2,
                    deliveryStatus: "retry_scheduled",
                    lastError: "host_not_ready: start_card initial grace before first inject",
                    payload: buildStartCardVariant(),
                  })
                : null;
          return delivery
            ? new Response(JSON.stringify(delivery), {
                status: 200,
                headers: { "content-type": "application/json" },
              })
            : new Response(null, { status: 204 });
        }
        if (url.includes("/v1/host-execution/next?host_session_id=")) {
          return new Response(null, { status: 204 });
        }
        if (url.endsWith("/v1/outbound/delivery-1/retry")) {
          retryRequests.push({
            scheduledAt: Date.now(),
            body: JSON.parse(String(init?.body ?? "{}")) as {
              next_attempt_at: string;
              last_error: string;
            },
          });
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/outbound/delivery-1/ack")) {
          return new Response(null, { status: 202 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      });
      globalThis.fetch = fetchMock as never;

      await apiKit.getService("loom-openclaw-peer")?.start?.();
      await ingestManagedTaskTurn(apiKit);
      await submitManagedTaskCandidate(apiKit, "tool-call-5");

      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(0);
      expect(retryRequests).toHaveLength(1);
      expect(retryRequests[0]?.body.last_error).toContain("initial grace");
      expect(Number(retryRequests[0]?.body.next_attempt_at) - retryRequests[0]!.scheduledAt).toBe(500);

      await vi.advanceTimersByTimeAsync(500);

      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(1);
      const gatewayCall = (apiKit.runCommandWithTimeout as unknown as { mock: { calls: unknown[] } }).mock
        .calls[0] as unknown[] | undefined;
      const gatewayArgs = gatewayCall?.[0];
      expect(Array.isArray(gatewayArgs)).toBe(true);
      if (!Array.isArray(gatewayArgs)) {
        throw new Error("gateway call argv missing");
      }
      expect(gatewayCall?.[1]).toMatchObject({ cwd: workspaceRoot });
      expect(gatewayArgs.slice(0, 4)).toEqual(["openclaw", "gateway", "call", "chat.inject"]);
      const params = JSON.parse(gatewayArgs[gatewayArgs.indexOf("--params") + 1] ?? "{}") as {
        sessionKey?: string;
        message?: string;
      };
      expect(params.sessionKey).toBe("session-1");
      expect(params.message).toContain("Managed task");
      expect(params.message).toContain("Verify visible transcript injection");
      expect(fetchMock.mock.calls.some(([input]) =>
        (typeof input === "string" ? input : input.toString()).endsWith("/v1/outbound/delivery-1/ack"),
      )).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("classifies start-card transcript misses, uses fast retries, parks quiescent, and wakes on new session activity", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-03-11T00:00:00.000Z"));
    try {
      const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
      writeBootstrapTicket(rootDir, "bridge-1");
      const apiKit = createMockApi(rootDir);
      apiKit.runCommandWithTimeout
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(successfulInjectResult("inject-2"));
      await plugin.register(apiKit.api as never);

      let outboundPolls = 0;
      const retryRequests: Array<{ scheduledAt: number; body: { next_attempt_at: string; last_error: string } }> = [];
      const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.endsWith("/v1/health")) {
          return new Response(
            JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/bootstrap")) {
          return new Response(
            JSON.stringify({
              bridge_instance_id: "bridge-1",
              credential_id: "cred-1",
              secret_ref: "secret-ref-1",
              rotation_epoch: 1,
              session_secret: "session-secret-1",
              issued_at: "1001",
              expires_at: null,
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/ingress/current-turn")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/capability-snapshot")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/semantic-bundle")) {
          return new Response(null, { status: 202 });
        }
        if (url.includes("/v1/outbound/next?host_session_id=")) {
          outboundPolls += 1;
          const delivery =
            outboundPolls <= 6
              ? buildOutboundDeliveryResponse({
                  deliveryId: "delivery-2",
                  attempts: outboundPolls,
                  deliveryStatus: outboundPolls === 1 ? "pending" : "retry_scheduled",
                  lastError:
                    outboundPolls === 1
                      ? null
                      : "host_not_ready: failed to write transcript: transcript file not found",
                  payload: buildStartCardVariant({
                    managed_task_ref: "task-2",
                    decision_token: "decision-2",
                    summary: "Wake the parked start card",
                    expected_outcome: "Wake from quiescent after new activity",
                  }),
                })
              : null;
          return delivery
            ? new Response(JSON.stringify(delivery), {
                status: 200,
                headers: { "content-type": "application/json" },
              })
            : new Response(null, { status: 204 });
        }
        if (url.includes("/v1/host-execution/next?host_session_id=")) {
          return new Response(null, { status: 204 });
        }
        if (url.endsWith("/v1/outbound/delivery-2/retry")) {
          retryRequests.push({
            scheduledAt: Date.now(),
            body: JSON.parse(String(init?.body ?? "{}")) as {
              next_attempt_at: string;
              last_error: string;
            },
          });
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/outbound/delivery-2/ack")) {
          return new Response(null, { status: 202 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      });
      globalThis.fetch = fetchMock as never;

      await apiKit.getService("loom-openclaw-peer")?.start?.();
      await ingestManagedTaskTurn(apiKit);
      await submitManagedTaskCandidate(apiKit, "tool-call-6");

      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(0);
      await vi.advanceTimersByTimeAsync(500);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1_000);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(2_000);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(3);
      await vi.advanceTimersByTimeAsync(4_000);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(4);

      expect(retryRequests).toHaveLength(5);
      expect(retryRequests.map((request) => Number(request.body.next_attempt_at) - request.scheduledAt)).toEqual([
        500,
        1_000,
        2_000,
        4_000,
        86_400_000,
      ]);
      expect(retryRequests[1]?.body.last_error).toBe(
        "host_not_ready: failed to write transcript: transcript file not found",
      );
      expect(
        apiKit.logs.some((entry) => entry.message === "bridge.peer.outbound_inject_failure_classified"),
      ).toBe(true);
      expect(
        apiKit.logs.some((entry) => entry.message === "bridge.peer.outbound_interactive_quiesced"),
      ).toBe(true);
      expect(
        apiKit.logs.some((entry) => entry.message === "bridge.peer.outbound_late_delivery_risk"),
      ).toBe(true);

      await vi.advanceTimersByTimeAsync(60_000);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(4);

      await ingestManagedTaskTurn(apiKit, {
        content: "One more detail for the managed task.",
        messageId: "host-message-2",
      });

      expect(retryRequests).toHaveLength(6);
      expect(retryRequests[5]?.body.last_error).toContain("woken_by_message_received");
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(5);
      expect(fetchMock.mock.calls.some(([input]) =>
        (typeof input === "string" ? input : input.toString()).endsWith("/v1/outbound/delivery-2/ack"),
      )).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps the default retry path for async notices while prefixing last_error with the failure class", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    apiKit.runCommandWithTimeout.mockResolvedValueOnce(hostNotReadyInjectResult());
    await plugin.register(apiKit.api as never);

    let outboundPolls = 0;
    const retryRequests: Array<{ body: { next_attempt_at: string; last_error: string } }> = [];
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        outboundPolls += 1;
        const delivery =
          outboundPolls === 1
            ? buildOutboundDeliveryResponse({
                deliveryId: "delivery-3",
                payload: buildResultSummaryVariant({
                  managed_task_ref: "task-3",
                  summary: "Render the async result summary.",
                }),
              })
            : null;
        return delivery
          ? new Response(JSON.stringify(delivery), {
              status: 200,
              headers: { "content-type": "application/json" },
            })
          : new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.endsWith("/v1/outbound/delivery-3/retry")) {
        retryRequests.push({
          body: JSON.parse(String(init?.body ?? "{}")) as {
            next_attempt_at: string;
            last_error: string;
          },
        });
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/outbound/delivery-3/ack")) {
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await ingestManagedTaskTurn(apiKit);
    await submitManagedTaskCandidate(apiKit, "tool-call-7");

    expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(1);
    expect(retryRequests).toHaveLength(1);
    expect(retryRequests[0]?.body.last_error).toBe(
      "host_not_ready: failed to write transcript: transcript file not found",
    );
    expect(
      apiKit.logs.some((entry) => entry.message === "bridge.peer.outbound_inject_failure_classified"),
    ).toBe(true);
  });

  it("delivers status_notice as async outbox text without caching a control surface", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    apiKit.runCommandWithTimeout.mockResolvedValueOnce(successfulInjectResult("inject-status-1"));
    await plugin.register(apiKit.api as never);

    let outboundPolls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        outboundPolls += 1;
        const delivery =
          outboundPolls === 1
            ? buildOutboundDeliveryResponse({
                deliveryId: "delivery-status-1",
                payload: buildStatusNoticeVariant({
                  managed_task_ref: "task-status-1",
                  headline: "Execute stage blocked",
                  notice_kind: "blocked",
                  summary: "Task could not enter execute because the host bridge is missing required agents.",
                }),
              })
            : null;
        return delivery
          ? new Response(JSON.stringify(delivery), {
              status: 200,
              headers: { "content-type": "application/json" },
            })
          : new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.endsWith("/v1/outbound/delivery-status-1/ack")) {
        return new Response(null, { status: 202 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    globalThis.fetch = fetchMock as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await ingestManagedTaskTurn(apiKit);
    await submitManagedTaskCandidate(apiKit, "tool-call-status-1");

    expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(1);
    const gatewayCall = apiKit.runCommandWithTimeout.mock.calls[0] as unknown[] | undefined;
    const gatewayArgs = gatewayCall?.[0];
    expect(Array.isArray(gatewayArgs)).toBe(true);
    if (!Array.isArray(gatewayArgs)) {
      throw new Error("gateway call argv missing");
    }
    expect(gatewayArgs.slice(0, 4)).toEqual(["openclaw", "gateway", "call", "chat.inject"]);
    const params = JSON.parse(gatewayArgs[gatewayArgs.indexOf("--params") + 1] ?? "{}") as {
      sessionKey?: string;
      message?: string;
    };
    expect(params.sessionKey).toBe("session-1");
    expect(params.message).toContain("Notice: Execute stage blocked");
    expect(params.message).not.toContain("Commands:");
    expect(fetchMock.mock.calls.some(([input]) =>
      (typeof input === "string" ? input : input.toString()).endsWith("/v1/outbound/delivery-status-1/ack"),
    )).toBe(true);

    const command = apiKit.getCommand("loom");
    await command?.handler({
      senderId: "user-1",
      channel: "webchat",
      channelId: "webchat",
      isAuthorizedSender: true,
      args: "probe",
      commandBody: "/loom probe",
      config: {},
      from: "user-1",
      to: "main",
      sessionKey: "session-1",
    });

    const projection = readCommandProbeProjection(rootDir);
    expect(projection.lastCommand?.resolvedHostSessionId).toBe("session-1");
    expect(projection.lastCommand?.latestControlSurfaceAtInvoke).toBeUndefined();
  });

  it("wakes a quiescent start card when the bridge becomes active again", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-03-11T00:00:00.000Z"));
    try {
      const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
      writeBootstrapTicket(rootDir, "bridge-1");
      const apiKit = createMockApi(rootDir);
      apiKit.runCommandWithTimeout
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(successfulInjectResult("inject-bridge-active"));
      await plugin.register(apiKit.api as never);

      let outboundPolls = 0;
      const retryRequests: Array<{ body: { next_attempt_at: string; last_error: string } }> = [];
      const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.endsWith("/v1/health")) {
          return new Response(
            JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/bootstrap")) {
          return new Response(
            JSON.stringify({
              bridge_instance_id: "bridge-1",
              credential_id: "cred-1",
              secret_ref: "secret-ref-1",
              rotation_epoch: 1,
              session_secret: "session-secret-1",
              issued_at: "1001",
              expires_at: null,
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/ingress/current-turn")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/capability-snapshot")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/semantic-bundle")) {
          return new Response(null, { status: 202 });
        }
        if (url.includes("/v1/outbound/next?host_session_id=")) {
          outboundPolls += 1;
          const delivery =
            outboundPolls <= 6
              ? buildOutboundDeliveryResponse({
                  deliveryId: "delivery-bridge-active",
                  attempts: outboundPolls,
                  deliveryStatus: outboundPolls === 1 ? "pending" : "retry_scheduled",
                  lastError:
                    outboundPolls === 1
                      ? null
                      : "host_not_ready: failed to write transcript: transcript file not found",
                  payload: buildStartCardVariant({
                    managed_task_ref: "task-bridge-active",
                    decision_token: "decision-bridge-active",
                    summary: "Wake after bridge reactivation",
                  }),
                })
              : null;
          return delivery
            ? new Response(JSON.stringify(delivery), {
                status: 200,
                headers: { "content-type": "application/json" },
              })
            : new Response(null, { status: 204 });
        }
        if (url.includes("/v1/host-execution/next?host_session_id=")) {
          return new Response(null, { status: 204 });
        }
        if (url.endsWith("/v1/outbound/delivery-bridge-active/retry")) {
          retryRequests.push({
            body: JSON.parse(String(init?.body ?? "{}")) as {
              next_attempt_at: string;
              last_error: string;
            },
          });
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/outbound/delivery-bridge-active/ack")) {
          return new Response(null, { status: 202 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      });
      globalThis.fetch = fetchMock as never;

      await apiKit.getService("loom-openclaw-peer")?.start?.();
      await ingestManagedTaskTurn(apiKit);
      await submitManagedTaskCandidate(apiKit, "tool-call-8");

      await vi.advanceTimersByTimeAsync(500);
      await vi.advanceTimersByTimeAsync(1_000);
      await vi.advanceTimersByTimeAsync(2_000);
      await vi.advanceTimersByTimeAsync(4_000);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(4);
      expect(retryRequests).toHaveLength(5);

      await apiKit.getService("loom-openclaw-peer")?.stop?.();
      await apiKit.getService("loom-openclaw-peer")?.start?.();

      expect(retryRequests).toHaveLength(6);
      expect(retryRequests[5]?.body.last_error).toContain("woken_by_bridge_active");
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(5);
      expect(fetchMock.mock.calls.some(([input]) =>
        (typeof input === "string" ? input : input.toString()).endsWith("/v1/outbound/delivery-bridge-active/ack"),
      )).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("wakes quiescent start cards on /loom help but not on /loom approve", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-03-11T00:00:00.000Z"));
    try {
      const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
      writeBootstrapTicket(rootDir, "bridge-1");
      const apiKit = createMockApi(rootDir);
      apiKit.runCommandWithTimeout
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(hostNotReadyInjectResult())
        .mockResolvedValueOnce(successfulInjectResult("inject-help"));
      await plugin.register(apiKit.api as never);

      let outboundPolls = 0;
      const retryRequests: Array<{ body: { next_attempt_at: string; last_error: string } }> = [];
      const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.endsWith("/v1/health")) {
          return new Response(
            JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/bootstrap")) {
          return new Response(
            JSON.stringify({
              bridge_instance_id: "bridge-1",
              credential_id: "cred-1",
              secret_ref: "secret-ref-1",
              rotation_epoch: 1,
              session_secret: "session-secret-1",
              issued_at: "1001",
              expires_at: null,
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/v1/ingress/current-turn")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/capability-snapshot")) {
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/ingress/semantic-bundle")) {
          return new Response(null, { status: 202 });
        }
        if (url.includes("/v1/control-surface/current?host_session_id=")) {
          return new Response(
            JSON.stringify({
              host_session_id: "session-1",
              surface_type: "start_card",
              managed_task_ref: "task-help-approve",
              decision_token: "decision-help-approve",
              allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.includes("/v1/outbound/next?host_session_id=")) {
          outboundPolls += 1;
          const delivery =
            outboundPolls <= 6
              ? buildOutboundDeliveryResponse({
                  deliveryId: "delivery-help-approve",
                  attempts: outboundPolls,
                  deliveryStatus: outboundPolls === 1 ? "pending" : "retry_scheduled",
                  lastError:
                    outboundPolls === 1
                      ? null
                      : "host_not_ready: failed to write transcript: transcript file not found",
                  payload: buildStartCardVariant({
                    managed_task_ref: "task-help-approve",
                    decision_token: "decision-help-approve",
                    summary: "Wake from /loom help only",
                  }),
                })
              : null;
          return delivery
            ? new Response(JSON.stringify(delivery), {
                status: 200,
                headers: { "content-type": "application/json" },
              })
            : new Response(null, { status: 204 });
        }
        if (url.includes("/v1/host-execution/next?host_session_id=")) {
          return new Response(null, { status: 204 });
        }
        if (url.endsWith("/v1/outbound/delivery-help-approve/retry")) {
          retryRequests.push({
            body: JSON.parse(String(init?.body ?? "{}")) as {
              next_attempt_at: string;
              last_error: string;
            },
          });
          return new Response(null, { status: 202 });
        }
        if (url.endsWith("/v1/outbound/delivery-help-approve/ack")) {
          return new Response(null, { status: 202 });
        }
        throw new Error(`unexpected fetch: ${url}`);
      });
      globalThis.fetch = fetchMock as never;

      await apiKit.getService("loom-openclaw-peer")?.start?.();
      await ingestManagedTaskTurn(apiKit);
      await submitManagedTaskCandidate(apiKit, "tool-call-9");

      await vi.advanceTimersByTimeAsync(500);
      await vi.advanceTimersByTimeAsync(1_000);
      await vi.advanceTimersByTimeAsync(2_000);
      await vi.advanceTimersByTimeAsync(4_000);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(4);
      expect(retryRequests).toHaveLength(5);

      const command = apiKit.getCommand("loom");
      const helpResult = await command?.handler({
        senderId: "user-1",
        channel: "webchat",
        channelId: "webchat",
        isAuthorizedSender: true,
        args: "help",
        commandBody: "/loom help",
        config: {},
        from: "user-1",
        to: "session-1",
        sessionKey: "session-1",
      });
      expect(helpResult).toMatchObject({
        text: expect.stringContaining("Current Loom control surface: start_card"),
      });
      expect(retryRequests).toHaveLength(6);
      expect(retryRequests[5]?.body.last_error).toContain("woken_by_loom_help");
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(5);

      const approveResult = await command?.handler({
        senderId: "user-1",
        channel: "webchat",
        channelId: "webchat",
        isAuthorizedSender: true,
        args: "approve",
        commandBody: "/loom approve",
        config: {},
        from: "user-1",
        to: "session-1",
        sessionKey: "session-1",
      });
      expect(approveResult).toMatchObject({
        text: expect.stringContaining("Submitted Loom action: approve_start"),
      });
      expect(retryRequests).toHaveLength(6);
      expect(apiKit.runCommandWithTimeout).toHaveBeenCalledTimes(5);
    } finally {
      vi.useRealTimers();
    }
  });

  it("marks internal tool results as internal transcript records", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    const decision = await apiKit.getHook("tool_result_persist")?.(
      {
        message: {
          role: "toolResult",
          toolName: "loom_emit_host_semantic_bundle",
          toolCallId: "tool-call-1",
        },
      },
      { toolName: "loom_emit_host_semantic_bundle", toolCallId: "tool-call-1" },
    );

    expect(decision).toMatchObject({
      message: {
        role: "toolResult",
        transcriptVisibility: "internal",
      },
    });
  });

  it("suppresses assistant transcript after a managed task candidate is submitted", async () => {
    const rootDir = mkdtempSync(join(tmpdir(), "loom-openclaw-plugin-"));
    writeBootstrapTicket(rootDir, "bridge-1");
    const apiKit = createMockApi(rootDir);
    await plugin.register(apiKit.api as never);

    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/v1/health")) {
        return new Response(
          JSON.stringify({ bridge_instance_id: "bridge-1", status: "ready" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/bootstrap")) {
        return new Response(
          JSON.stringify({
            bridge_instance_id: "bridge-1",
            credential_id: "cred-1",
            secret_ref: "secret-ref-1",
            rotation_epoch: 1,
            session_secret: "session-secret-1",
            issued_at: "1001",
            expires_at: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/v1/ingress/current-turn")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/capability-snapshot")) {
        return new Response(null, { status: 202 });
      }
      if (url.endsWith("/v1/ingress/semantic-bundle")) {
        return new Response(null, { status: 202 });
      }
      if (url.includes("/v1/outbound/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      if (url.includes("/v1/host-execution/next?host_session_id=")) {
        return new Response(null, { status: 204 });
      }
      throw new Error(`unexpected fetch: ${url}`);
    }) as never;

    await apiKit.getService("loom-openclaw-peer")?.start?.();
    await apiKit.getHook("message_received")?.(
      {
        content: "Please start a managed task.",
        metadata: { messageId: "host-message-1" },
      },
      { sessionKey: "session-1", conversationId: "session-1" },
    );

    const descriptor = apiKit.getToolDescriptor() as {
      execute: (toolCallId: string, params: unknown) => Promise<unknown>;
    };
    await executeToolBundle(descriptor, "tool-call-4", {
      schema_version: { major: 0, minor: 1 },
      input_ref: "host-message-1",
      source_model_ref: "host-model",
      issued_at: "1010",
      decisions: [
        {
          decision_kind: "interaction_lane",
          decision_source: "host_model",
          confidence: 0.98,
          rationale: "The user explicitly asked to start a managed task.",
          payload: {
            interaction_lane: "managed_task_candidate",
            summary: "Start the bridge analysis task",
          },
        },
        {
          decision_kind: "task_activation_reason",
          decision_source: "host_model",
          confidence: 0.96,
          rationale: "This is an explicit managed-task request.",
          payload: {
            task_activation_reason: "explicit_start_task",
          },
        },
        {
          decision_kind: "managed_task_class",
          decision_source: "host_model",
          confidence: 0.94,
          rationale: "The task is complex but bounded.",
          payload: {
            managed_task_class: "complex",
          },
        },
        {
          decision_kind: "work_horizon",
          decision_source: "host_model",
          confidence: 0.92,
          rationale: "The task improves an existing implementation.",
          payload: {
            work_horizon: "improvement",
          },
        },
      ],
    });

    expect(
      await apiKit.getHook("message_sending")?.(
        {},
        { sessionKey: "session-1", runId: "run-1" },
      ),
    ).toMatchObject({ cancel: true });

    expect(
      await apiKit.getHook("before_message_write")?.(
        {
          message: {
            role: "assistant",
            content: [{ type: "text", text: "I created the candidate task." }],
          },
        },
        { sessionKey: "session-1", runId: "run-1" },
      ),
    ).toMatchObject({ block: true });
  });
});

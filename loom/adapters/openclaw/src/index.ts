import { createHash, randomUUID } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import type { OpenClawPluginApi } from "openclaw/plugin-sdk";

import { BridgeHttpError, createLoomBridgeClient } from "./client.js";
import {
  mapHostSemanticBundleToControlAction,
  mapHostSemanticBundleToSemanticDecision,
} from "./mapping.js";
import { renderPayload } from "./render.js";
import type {
  BridgeBootstrapMaterial,
  BridgeSessionCredential,
  BridgeStatus,
  CurrentTurnEnvelope,
  HostExecutionCommand,
  HostSemanticBundle,
  HostSubagentLifecycleEnvelope,
  HostSessionId,
  TurnBinding,
} from "./types.js";
import type { HostCapabilitySnapshot } from "./rustWireTypes.js";

export type LoomOpenClawConfig = {
  bridge: {
    baseUrl: string;
  };
};

const DEFAULT_BRIDGE_URL = "http://127.0.0.1:6417";
const ADAPTER_ID = "loom-openclaw";
const TOOL_NAME = "loom_emit_host_semantic_bundle";
const SERVICE_ID = "loom-openclaw-peer";
const BOOTSTRAP_TICKET_RELATIVE_PATH = "runtime/loom/bootstrap/openclaw/bootstrap-ticket.json";
const DEDUPE_WINDOW = "PT10M";
const GATEWAY_CALL_TIMEOUT_MS = 10_000;
const INTERNAL_EXECUTION_MARKER = "[loom-host-execution]";
const INTERNAL_EXECUTION_AGENT = "main";
const DECISION_SOURCES = [
  "host_model",
  "pack_default",
  "system_reconsideration",
  "user_control_action",
  "adapter_fallback",
] as const;
const INTERACTION_LANES = ["chat", "managed_task_candidate", "managed_task_active"] as const;
const TASK_ACTIVATION_REASONS = [
  "explicit_user_request",
  "scope_change",
  "capability_drift",
  "review_escalation",
] as const;
const MANAGED_TASK_CLASSES = ["complex", "huge", "max"] as const;
const WORK_HORIZONS = ["maintenance", "improvement", "extension", "disruption"] as const;
const CONTROL_ACTIONS = [
  "approve_start",
  "modify_candidate",
  "cancel_candidate",
  "keep_current_task",
  "replace_active",
  "approve_request",
  "reject_request",
  "pause_task",
  "resume_task",
  "cancel_task",
  "request_review",
  "request_horizon_reconsideration",
  "request_task_change",
] as const;

type CommandDispatchContext = {
  commandId: string;
  hostSessionId: string;
  managedTaskRef: string;
  runRef: string;
  roleKind: HostExecutionCommand["role_kind"];
  hostAgentId: string;
  helperSessionKey: string;
};

function resolveBridgeBaseUrl(api: OpenClawPluginApi): string {
  const configured = api.getConfig?.<string>("bridge.baseUrl");
  if (configured) return configured;
  const pluginConfig = api.pluginConfig as LoomOpenClawConfig | undefined;
  return pluginConfig?.bridge?.baseUrl ?? DEFAULT_BRIDGE_URL;
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function nowTimestamp(): string {
  return Date.now().toString();
}

function newId(prefix: string): string {
  return `${prefix}-${randomUUID()}`;
}

function hashJson(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function buildGatewayCallArgs(method: string, params: Record<string, unknown>): string[] {
  return [
    "openclaw",
    "gateway",
    "call",
    method,
    "--json",
    "--params",
    JSON.stringify(params),
  ];
}

function executionHelperSessionKey(hostSessionId: string): string {
  const safeSuffix = hostSessionId.replace(/[^a-zA-Z0-9:_-]/g, "_");
  return `agent:${INTERNAL_EXECUTION_AGENT}:loom-exec:${safeSuffix}`;
}

function isInternalExecutionText(text: string): boolean {
  return text.trimStart().startsWith(INTERNAL_EXECUTION_MARKER);
}

function extractJsonPayload(text: string): unknown {
  const trimmed = text.trim();
  if (!trimmed) {
    throw new Error("gateway call produced empty stdout");
  }
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    const start = trimmed.indexOf("{");
    const end = trimmed.lastIndexOf("}");
    if (start === -1 || end <= start) {
      throw new Error(`gateway call returned non-json stdout: ${trimmed}`);
    }
    return JSON.parse(trimmed.slice(start, end + 1)) as unknown;
  }
}

function readBootstrapMaterial(api: OpenClawPluginApi): BridgeBootstrapMaterial {
  const ticketPath = api.resolvePath(BOOTSTRAP_TICKET_RELATIVE_PATH);
  if (!existsSync(ticketPath)) {
    throw new Error(`bootstrap material missing: ${ticketPath}`);
  }
  return JSON.parse(readFileSync(ticketPath, "utf8")) as BridgeBootstrapMaterial;
}

function resolveCanonicalSession(ctx: Record<string, unknown>, event?: Record<string, unknown>): string | undefined {
  const candidates = [
    ctx.sessionKey,
    ctx.sessionId,
    event?.sessionKey,
    event?.threadId,
    ctx.conversationId,
    ctx.threadId,
    event?.conversationId,
  ];
  return candidates.find(
    (candidate): candidate is string =>
      typeof candidate === "string" && candidate.trim().length > 0,
  );
}

function resolveDefaultAgentId(api: OpenClawPluginApi): string {
  const config = (api as { config?: Record<string, unknown> }).config;
  const agents =
    config &&
    typeof config === "object" &&
    "agents" in config &&
    config.agents &&
    typeof config.agents === "object" &&
    "list" in (config.agents as Record<string, unknown>) &&
    Array.isArray((config.agents as Record<string, unknown>).list)
      ? ((config.agents as Record<string, unknown>).list as Array<Record<string, unknown>>)
      : [];
  const selected =
    agents.find((entry) => entry?.default === true && nonEmptyString(entry.id)) ??
    agents.find((entry) => nonEmptyString(entry.id));
  return nonEmptyString(selected?.id) ?? "main";
}

function resolveMessagePeerKind(ctx: Record<string, unknown>, event: Record<string, unknown>): "direct" | "group" | "channel" {
  const metadata =
    typeof event.metadata === "object" && event.metadata !== null
      ? (event.metadata as Record<string, unknown>)
      : {};
  const candidates = [
    ctx.chatType,
    ctx.peerKind,
    ctx.groupId ? "group" : undefined,
    metadata.chatType,
    metadata.peerKind,
  ];
  for (const candidate of candidates) {
    if (candidate === "group" || candidate === "channel" || candidate === "direct") {
      return candidate;
    }
  }
  return "direct";
}

function resolveSessionFromMessageContext(
  api: OpenClawPluginApi,
  ctx: Record<string, unknown>,
  event: Record<string, unknown>,
): string | undefined {
  const channelId = nonEmptyString(ctx.channelId);
  const conversationId =
    nonEmptyString(ctx.conversationId) ??
    nonEmptyString(event.conversationId) ??
    nonEmptyString(extractHostMessageRef(event)) ??
    nonEmptyString(event.from);
  if (!channelId || !conversationId) {
    return undefined;
  }

  const routing = (api.runtime as Record<string, unknown> | undefined)?.channel;
  const helpers =
    routing && typeof routing === "object"
      ? ((routing as Record<string, unknown>).routing as Record<string, unknown> | undefined)
      : undefined;
  const resolveAgentRoute = helpers?.resolveAgentRoute;
  if (typeof resolveAgentRoute === "function") {
    try {
      const route = resolveAgentRoute({
        cfg: (api as { config?: unknown }).config ?? {},
        channel: channelId,
        accountId: nonEmptyString(ctx.accountId) ?? null,
        peer: {
          kind: resolveMessagePeerKind(ctx, event),
          id: conversationId,
        },
      }) as Record<string, unknown> | undefined;
      return nonEmptyString(route?.sessionKey);
    } catch (error) {
      api.logger.warn?.("bridge.peer.session_resolution_failed", {
        channel_id: channelId,
        conversation_id: conversationId,
        error: error instanceof Error ? error.message : error,
      });
    }
  }

  const buildAgentSessionKey = helpers?.buildAgentSessionKey;
  if (typeof buildAgentSessionKey === "function") {
    try {
      const sessionKey = buildAgentSessionKey({
        agentId: resolveDefaultAgentId(api),
        channel: channelId,
        accountId: nonEmptyString(ctx.accountId) ?? null,
        peer: {
          kind: resolveMessagePeerKind(ctx, event),
          id: conversationId,
        },
      });
      return nonEmptyString(sessionKey);
    } catch (error) {
      api.logger.warn?.("bridge.peer.session_key_build_failed", {
        channel_id: channelId,
        conversation_id: conversationId,
        error: error instanceof Error ? error.message : error,
      });
    }
  }

  return undefined;
}

function extractHostMessageRef(event: Record<string, unknown>): string | undefined {
  const metadata =
    typeof event.metadata === "object" && event.metadata !== null
      ? (event.metadata as Record<string, unknown>)
      : {};
  const candidates = [metadata.messageId, metadata.message_id, event.messageId, event.id];
  return candidates.find(
    (candidate): candidate is string =>
      typeof candidate === "string" && candidate.trim().length > 0,
  );
}

function extractTextContent(content: unknown): string {
  if (typeof content === "string") {
    return content;
  }
  if (Array.isArray(content)) {
    return content
      .map((item) => {
        if (typeof item === "string") return item;
        if (item && typeof item === "object" && "text" in item && typeof item.text === "string") {
          return item.text;
        }
        return "";
      })
      .filter(Boolean)
      .join("\n");
  }
  if (content && typeof content === "object" && "text" in content && typeof content.text === "string") {
    return content.text;
  }
  return "";
}

function extractPathLikeReferences(text: string): string[] {
  const matches =
    text.match(/(?:\/[^\s,;:]+)+|(?:\.\/)?(?:[A-Za-z0-9._-]+\/)+[A-Za-z0-9._-]+/g) ?? [];
  return [
    ...new Set(
      matches
        .map((entry) => entry.trim().replace(/[),.;:]+$/g, ""))
        .map((entry) => (entry.startsWith("./") ? entry.slice(2) : entry))
        .filter(Boolean),
    ),
  ];
}

function lastNonEmptyLine(text: string): string {
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  return lines.at(-1) ?? text.trim();
}

function extractSessionTranscriptSummary(payload: unknown): { outputSummary: string; artifactRefs: string[] } {
  const messages = Array.isArray((payload as { messages?: unknown[] })?.messages)
    ? ((payload as { messages: unknown[] }).messages as unknown[])
    : [];
  const assistantMessages = messages.filter((message) => isAssistantMessage(message));
  const lastAssistant = assistantMessages.at(-1) as Record<string, unknown> | undefined;
  const outputSummary = lastAssistant ? extractTextContent(lastAssistant.content ?? lastAssistant.text) : "";
  return {
    outputSummary: outputSummary.trim() || lastNonEmptyLine(JSON.stringify(payload)),
    artifactRefs: extractPathLikeReferences(outputSummary),
  };
}

function subagentStatusFromOutcome(outcome: unknown): "completed" | "failed" | "timed_out" | "cancelled" {
  if (!outcome || typeof outcome !== "object") {
    return "failed";
  }
  const normalized = nonEmptyString((outcome as { status?: unknown }).status)?.toLowerCase();
  switch (normalized) {
    case "ok":
    case "completed":
    case "success":
      return "completed";
    case "timeout":
    case "timed_out":
      return "timed_out";
    case "cancelled":
    case "aborted":
      return "cancelled";
    default:
      return "failed";
  }
}

function extractInboundText(event: Record<string, unknown>): string {
  const candidates = [
    event.content,
    event.bodyForAgent,
    event.body,
    event.message && typeof event.message === "object"
      ? (event.message as Record<string, unknown>).content
      : undefined,
    event.message && typeof event.message === "object"
      ? (event.message as Record<string, unknown>).text
      : undefined,
  ];
  for (const candidate of candidates) {
    const text = extractTextContent(candidate);
    if (text.trim()) {
      return text;
    }
  }
  return "";
}

function buildCurrentTurn(hostSessionId: string, hostMessageRef: string | undefined, text: string): CurrentTurnEnvelope {
  return {
    meta: {
      ingress_id: newId("ingress"),
      received_at: nowTimestamp(),
      causation_id: null,
      correlation_id: newId("corr"),
      dedupe_window: DEDUPE_WINDOW,
    },
    host_session_id: hostSessionId,
    host_message_ref: hostMessageRef ?? null,
    text,
    workspace_ref: "/Users/codez/.openclaw",
    repo_ref: "openclaw",
  };
}

function buildCapabilitySnapshot(hostSessionId: string, api: OpenClawPluginApi): HostCapabilitySnapshot {
  const workspaceRoot = api.resolvePath(".");
  const config = (api as { config?: Record<string, unknown> }).config;
  const agents = readAgentEntries(config);
  const currentAgent = selectCurrentAgent(agents);
  const availableAgents = currentAgent ? computeSpawnableChildAgents(config, currentAgent, agents) : [];
  return {
    capability_snapshot_ref: newId("cap"),
    host_session_id: hostSessionId,
    allowed_tools: [TOOL_NAME],
    readable_roots: [workspaceRoot],
    writable_roots: [workspaceRoot],
    secret_classes: ["repo"],
    max_budget_band: "standard",
    available_agent_ids: availableAgents,
    supports_spawn_agents: availableAgents.length > 0,
    supports_pause: true,
    supports_resume: true,
    supports_interrupt: true,
    recorded_at: nowTimestamp(),
  };
}

function readAgentEntries(config: unknown): Array<Record<string, unknown>> {
  if (
    !config ||
    typeof config !== "object" ||
    !("agents" in config) ||
    !(config.agents && typeof config.agents === "object") ||
    !("list" in (config.agents as Record<string, unknown>)) ||
    !Array.isArray((config.agents as Record<string, unknown>).list)
  ) {
    return [];
  }
  return (config.agents as Record<string, unknown>).list as Array<Record<string, unknown>>;
}

function selectCurrentAgent(agents: Array<Record<string, unknown>>): Record<string, unknown> | undefined {
  return (
    agents.find((entry) => entry.default === true) ??
    agents.find((entry) => nonEmptyString(entry.id) === "main")
  );
}

function toolList(entry: unknown): string[] | null {
  if (!Array.isArray(entry)) {
    return null;
  }
  return entry
    .map((value) => nonEmptyString(value))
    .filter((value): value is string => Boolean(value));
}

function canAgentSpawn(config: unknown, agent: Record<string, unknown>): boolean {
  const agentToAgentEnabled =
    !config ||
    typeof config !== "object" ||
    !("tools" in config) ||
    !(config.tools && typeof config.tools === "object") ||
    !("agentToAgent" in (config.tools as Record<string, unknown>)) ||
    !((config.tools as Record<string, unknown>).agentToAgent &&
      typeof (config.tools as Record<string, unknown>).agentToAgent === "object")
      ? true
      : (config.tools as { agentToAgent?: { enabled?: unknown } }).agentToAgent?.enabled !== false;
  if (!agentToAgentEnabled) {
    return false;
  }
  const tools =
    agent.tools && typeof agent.tools === "object" ? (agent.tools as Record<string, unknown>) : undefined;
  const allow = toolList(tools?.allow);
  const deny = new Set(toolList(tools?.deny) ?? []);
  if (deny.has("sessions_spawn")) {
    return false;
  }
  if (allow) {
    return allow.includes("sessions_spawn");
  }
  return true;
}

function computeSpawnableChildAgents(
  config: unknown,
  currentAgent: Record<string, unknown>,
  agents: Array<Record<string, unknown>>,
): string[] {
  if (!canAgentSpawn(config, currentAgent)) {
    return [];
  }
  const currentAgentId = nonEmptyString(currentAgent.id);
  const allowed =
    currentAgent.subagents &&
    typeof currentAgent.subagents === "object" &&
    Array.isArray((currentAgent.subagents as Record<string, unknown>).allowAgents)
      ? ((currentAgent.subagents as Record<string, unknown>).allowAgents as unknown[])
          .map((value) => nonEmptyString(value))
          .filter((value): value is string => Boolean(value))
      : [];
  const definedAgents = new Set(
    agents.map((entry) => nonEmptyString(entry.id)).filter((value): value is string => Boolean(value)),
  );
  return [...new Set(allowed)]
    .filter((agentId) => agentId !== currentAgentId)
    .filter((agentId) => definedAgents.has(agentId));
}

function capabilityFingerprint(snapshot: HostCapabilitySnapshot): string {
  return hashJson({
    host_session_id: snapshot.host_session_id,
    allowed_tools: [...snapshot.allowed_tools].sort(),
    readable_roots: [...snapshot.readable_roots].sort(),
    writable_roots: [...snapshot.writable_roots].sort(),
    secret_classes: [...snapshot.secret_classes].sort(),
    max_budget_band: snapshot.max_budget_band,
    available_agent_ids: [...snapshot.available_agent_ids].sort(),
    supports_spawn_agents: snapshot.supports_spawn_agents,
    supports_pause: snapshot.supports_pause,
    supports_resume: snapshot.supports_resume,
    supports_interrupt: snapshot.supports_interrupt,
  });
}

function buildGovernancePrompt(): string {
  return [
    "Loom governance is active for this host turn.",
    `Before any user-visible answer, call ${TOOL_NAME} exactly once with a HostSemanticBundle.`,
    "The bundle must use schema_version { major: 0, minor: 1 }.",
    "Each bundle must emit either an interaction_lane decision or a control_action decision.",
    "The only safe fallback is interaction_lane=chat.",
    "For an explicit managed-task start request, emit interaction_lane=managed_task_candidate plus task_activation_reason, managed_task_class, and work_horizon in the same bundle.",
    "For an explicit structured governance reply, emit control_action directly and do not invent an interaction_lane fallback.",
    "Use these exact payload keys: interaction_lane, task_activation_reason, managed_task_class, work_horizon.",
    'Minimal chat fallback example: {"decision_kind":"interaction_lane","decision_source":"adapter_fallback","confidence":0.2,"rationale":"fallback to chat","payload":{"interaction_lane":"chat"}}',
    'Minimal managed-task start example: {"decision_kind":"interaction_lane","decision_source":"host_model","confidence":0.95,"rationale":"explicit managed task request","payload":{"interaction_lane":"managed_task_candidate","summary":"Start a managed task"}}',
    'Minimal control action example: {"decision_kind":"control_action","decision_source":"user_control_action","confidence":0.99,"rationale":"explicit approval from the control surface","payload":{"action_kind":"approve_start","managed_task_ref":"task-1","decision_token":"start-win-001"}}',
    "Do not infer control actions from free text.",
  ].join("\n");
}

function stringListSchema() {
  return {
    type: "array",
    items: { type: "string" },
  };
}

function requirementItemsSchema() {
  return {
    type: "array",
    items: {
      type: "object",
      additionalProperties: false,
      properties: {
        text: { type: "string" },
        origin: { type: "string" },
      },
      required: ["text"],
    },
  };
}

function baseDecisionSchema(kind: string, payload: Record<string, unknown>) {
  return {
    type: "object",
    additionalProperties: false,
    properties: {
      decision_kind: { const: kind },
      decision_source: { enum: [...DECISION_SOURCES] },
      confidence: { type: "number" },
      rationale: { type: "string" },
      payload,
    },
    required: ["decision_kind", "decision_source", "confidence", "rationale", "payload"],
  };
}

function hostTaskShapeSchema() {
  return {
    type: "object",
    additionalProperties: false,
    properties: {
      managed_task_ref: { type: "string" },
      title: { type: "string" },
      summary: { type: "string" },
      expected_outcome: { type: "string" },
      requirement_items: requirementItemsSchema(),
      workspace_ref: { type: "string" },
      repo_ref: { type: "string" },
      allowed_roots: stringListSchema(),
      secret_classes: stringListSchema(),
    },
  };
}

function hostSemanticDecisionSchemas() {
  const taskShape = hostTaskShapeSchema();
  return [
    baseDecisionSchema("interaction_lane", {
      ...taskShape,
      properties: {
        ...taskShape.properties,
        interaction_lane: { enum: [...INTERACTION_LANES] },
      },
      required: ["interaction_lane"],
    }),
    baseDecisionSchema("task_activation_reason", {
      type: "object",
      additionalProperties: false,
      properties: {
        task_activation_reason: { enum: [...TASK_ACTIVATION_REASONS] },
      },
      required: ["task_activation_reason"],
    }),
    baseDecisionSchema("managed_task_class", {
      type: "object",
      additionalProperties: false,
      properties: {
        managed_task_class: { enum: [...MANAGED_TASK_CLASSES] },
      },
      required: ["managed_task_class"],
    }),
    baseDecisionSchema("work_horizon", {
      type: "object",
      additionalProperties: false,
      properties: {
        work_horizon: { enum: [...WORK_HORIZONS] },
      },
      required: ["work_horizon"],
    }),
    baseDecisionSchema("task_change", {
      type: "object",
      additionalProperties: false,
      properties: {
        summary: { type: "string" },
        expected_outcome: { type: "string" },
        requirement_items: requirementItemsSchema(),
        allowed_roots: stringListSchema(),
        secret_classes: stringListSchema(),
        workspace_ref: { type: "string" },
        repo_ref: { type: "string" },
        rationale: { type: "string" },
      },
    }),
    baseDecisionSchema("control_action", {
      type: "object",
      additionalProperties: false,
      properties: {
        action_kind: { enum: [...CONTROL_ACTIONS] },
        decision_token: { type: "string" },
        managed_task_ref: { type: "string" },
        source_decision_ref: { type: "string" },
        payload: {
          type: "object",
          additionalProperties: false,
          properties: {
            title: { type: "string" },
            summary: { type: "string" },
            expected_outcome: { type: "string" },
            requirement_items: requirementItemsSchema(),
            allowed_roots: stringListSchema(),
            secret_classes: stringListSchema(),
            workspace_ref: { type: "string" },
            repo_ref: { type: "string" },
            rationale: { type: "string" },
          },
        },
      },
      required: ["action_kind"],
    }),
  ];
}

function isAssistantMessage(message: unknown): boolean {
  return Boolean(
    message &&
      typeof message === "object" &&
      "role" in message &&
      message.role === "assistant",
  );
}

function containsInternalToolCall(message: unknown): boolean {
  if (!message || typeof message !== "object") return false;
  const record = message as Record<string, unknown>;
  const content = Array.isArray(record.content) ? record.content : [];
  return content.some(
    (item) =>
      item &&
      typeof item === "object" &&
      ("name" in item || "toolName" in item) &&
      ((item as Record<string, unknown>).name === TOOL_NAME ||
        (item as Record<string, unknown>).toolName === TOOL_NAME),
  );
}

class LoomOpenClawRuntime {
  private bridgeStatus: BridgeStatus = "disconnected";
  private credential: BridgeSessionCredential | null = null;
  private latestBridgeInstanceId: string | null = null;
  private readonly turnsBySession = new Map<string, TurnBinding>();
  private readonly pendingSemanticSessions = new Set<string>();
  private readonly suppressAssistantSessions = new Set<string>();
  private readonly capabilityDigestBySession = new Map<string, string>();
  private readonly executionSessions = new Set<string>();
  private readonly dispatchContextByHelperSession = new Map<string, CommandDispatchContext>();
  private readonly dispatchContextByChildSession = new Map<string, CommandDispatchContext>();
  private readonly drainingExecutionSessions = new Set<string>();

  constructor(
    private readonly api: OpenClawPluginApi,
    private readonly client = createLoomBridgeClient(resolveBridgeBaseUrl(api), {
      adapterId: ADAPTER_ID,
      getCredential: () => this.credential,
    }),
  ) {}

  getBridgeStatus(): BridgeStatus {
    return this.bridgeStatus;
  }

  async startPeer(): Promise<void> {
    if (this.bridgeStatus === "active" || this.bridgeStatus === "bootstrapping") {
      return;
    }
    this.api.logger.info?.("bridge.peer.connecting", { baseUrl: resolveBridgeBaseUrl(this.api) });
    this.bridgeStatus = "connecting";
    try {
      const health = await this.client.health();
      const material = readBootstrapMaterial(this.api);
      if (material.bridge_instance_id !== health.bridge_instance_id) {
        this.failClosed("bootstrap material bridge_instance_id mismatch");
        return;
      }
      this.bridgeStatus = "bootstrapping";
      this.api.logger.info?.("bridge.peer.bootstrap_started", {
        bridge_instance_id: health.bridge_instance_id,
      });
      const ack = await this.client.bootstrap(material);
      this.credential = {
        bridge_instance_id: ack.bridge_instance_id,
        adapter_id: ADAPTER_ID,
        secret_ref: ack.secret_ref,
        rotation_epoch: ack.rotation_epoch,
        session_secret: ack.session_secret,
      };
      this.latestBridgeInstanceId = ack.bridge_instance_id;
      this.bridgeStatus = "active";
      this.api.logger.info?.("bridge.peer.bootstrap_succeeded", {
        bridge_instance_id: ack.bridge_instance_id,
        secret_ref: ack.secret_ref,
      });
    } catch (error) {
      this.failClosed(error instanceof Error ? error.message : "bootstrap failed", error);
    }
  }

  stopPeer(): void {
    this.credential = null;
    this.bridgeStatus = "disconnected";
    this.executionSessions.clear();
    this.dispatchContextByHelperSession.clear();
    this.dispatchContextByChildSession.clear();
    this.drainingExecutionSessions.clear();
  }

  private async ensurePeerReady(): Promise<boolean> {
    if (this.bridgeStatus === "active") {
      return true;
    }
    if (this.bridgeStatus === "disconnected" || this.bridgeStatus === "reconnect_required") {
      await this.startPeer();
    }
    return this.getBridgeStatus() === "active";
  }

  private failClosed(reason: string, error?: unknown): void {
    this.credential = null;
    this.bridgeStatus = "fail_closed";
    this.api.logger.error?.("bridge.peer.fail_closed", {
      reason,
      error: error instanceof Error ? error.message : error,
    });
  }

  private requestReconnect(reason: string, error?: unknown): void {
    this.credential = null;
    this.bridgeStatus = "reconnect_required";
    this.api.logger.warn?.("bridge.peer.reconnect_requested", {
      reason,
      error: error instanceof Error ? error.message : error,
    });
  }

  private shouldReconnect(error: unknown): boolean {
    if (error instanceof BridgeHttpError) {
      return error.status === 401 || error.status === 403;
    }
    return error instanceof Error;
  }

  private handleOperationFailure(reason: string, error: unknown): void {
    if (this.shouldReconnect(error)) {
      this.requestReconnect(reason, error);
      return;
    }
    this.api.logger.warn?.("bridge.peer.operation_failed", {
      reason,
      error: error instanceof Error ? error.message : error,
    });
  }

  rememberTurn(turn: TurnBinding): void {
    this.turnsBySession.set(turn.hostSessionId, turn);
    this.pendingSemanticSessions.add(turn.hostSessionId);
    this.suppressAssistantSessions.delete(turn.hostSessionId);
  }

  latestTurn(hostSessionId: string): TurnBinding | undefined {
    return this.turnsBySession.get(hostSessionId);
  }

  needsSemantic(hostSessionId: string): boolean {
    return this.pendingSemanticSessions.has(hostSessionId);
  }

  shouldSuppressAssistant(hostSessionId: string): boolean {
    return this.suppressAssistantSessions.has(hostSessionId);
  }

  isExecutionSession(sessionKey: string): boolean {
    return this.executionSessions.has(sessionKey);
  }

  async ingestCurrentTurn(hostSessionId: string, hostMessageRef: string | undefined, text: string): Promise<void> {
    if (this.isExecutionSession(hostSessionId)) {
      return;
    }
    const turn = buildCurrentTurn(hostSessionId, hostMessageRef, text);
    this.rememberTurn({
      hostSessionId,
      hostMessageRef,
      ingressId: turn.meta.ingress_id,
      correlationId: turn.meta.correlation_id,
      receivedAt: turn.meta.received_at,
      text,
    });
    if (!(await this.ensurePeerReady())) {
      return;
    }
    try {
      await this.client.postCurrentTurn(turn);
    } catch (error) {
      this.handleOperationFailure("current_turn_post_failed", error);
    }
  }

  async syncCapabilities(hostSessionId: string): Promise<void> {
    if (!(await this.ensurePeerReady())) {
      return;
    }
    const snapshot = buildCapabilitySnapshot(hostSessionId, this.api);
    const digest = capabilityFingerprint(snapshot);
    if (this.capabilityDigestBySession.get(hostSessionId) === digest) {
      return;
    }
    try {
      await this.client.postCapabilitySnapshot(snapshot);
      this.capabilityDigestBySession.set(hostSessionId, digest);
      this.api.logger.info?.("bridge.peer.capability_sync_succeeded", {
        host_session_id: hostSessionId,
      });
      await this.drainHostExecution(hostSessionId);
    } catch (error) {
      this.handleOperationFailure("capability_sync_failed", error);
    }
  }

  async drainHostExecution(hostSessionId: string): Promise<void> {
    if (!(await this.ensurePeerReady())) {
      return;
    }
    if (this.drainingExecutionSessions.has(hostSessionId)) {
      return;
    }
    this.drainingExecutionSessions.add(hostSessionId);
    try {
      while (true) {
        let command;
        try {
          command = await this.client.nextHostExecution(hostSessionId);
        } catch (error) {
          this.handleOperationFailure("host_execution_poll_failed", error);
          return;
        }
        if (!command) {
          return;
        }
        try {
          await this.dispatchHostExecution(command);
          await this.client.ackHostExecution(command.command_id);
        } catch (error) {
          this.handleOperationFailure("host_execution_dispatch_failed", error);
          return;
        }
      }
    } finally {
      this.drainingExecutionSessions.delete(hostSessionId);
    }
  }

  private async dispatchHostExecution(command: HostExecutionCommand): Promise<void> {
    const runner = this.api.runtime?.system?.runCommandWithTimeout;
    if (typeof runner !== "function") {
      throw new Error("host runtime missing runCommandWithTimeout");
    }
    const helperSessionKey = executionHelperSessionKey(command.host_session_id);
    const dispatchContext = {
      commandId: command.command_id,
      hostSessionId: command.host_session_id,
      managedTaskRef: command.managed_task_ref,
      runRef: command.run_ref,
      roleKind: command.role_kind,
      hostAgentId: command.host_agent_id,
      helperSessionKey,
    };
    this.executionSessions.add(helperSessionKey);
    this.dispatchContextByHelperSession.set(helperSessionKey, dispatchContext);
    const message = [
      INTERNAL_EXECUTION_MARKER,
      `command_id=${command.command_id}`,
      `managed_task_ref=${command.managed_task_ref}`,
      `role_kind=${command.role_kind}`,
      "",
      "Use the sessions_spawn tool exactly once with these values:",
      `agentId: ${command.host_agent_id}`,
      `label: ${command.label}`,
      "runtime: subagent",
      "",
      "Task prompt:",
      command.prompt,
      "",
      "After the sessions_spawn call, stop. Do not produce a user-facing summary.",
    ].join("\n");
    const result = await runner(
      buildGatewayCallArgs("chat.send", {
        sessionKey: helperSessionKey,
        message,
        deliver: false,
        idempotencyKey: `loom-exec-${command.command_id}`,
        timeoutMs: GATEWAY_CALL_TIMEOUT_MS,
      }),
      {
        timeoutMs: GATEWAY_CALL_TIMEOUT_MS,
        cwd: this.api.resolvePath("."),
      },
    );
    if (result.code !== 0) {
      this.cleanupDispatchTracking(dispatchContext);
      throw new Error(
        `gateway chat.send failed with exit ${String(result.code)}: ${result.stderr || result.stdout || "unknown error"}`,
      );
    }
    const parsed = extractJsonPayload(result.stdout) as { ok?: boolean };
    if (parsed.ok !== true) {
      this.cleanupDispatchTracking(dispatchContext);
      throw new Error(`gateway chat.send returned invalid payload: ${result.stdout}`);
    }
  }

  private cleanupDispatchTracking(dispatch: CommandDispatchContext, childSessionKey?: string): void {
    if (childSessionKey) {
      this.dispatchContextByChildSession.delete(childSessionKey);
      this.executionSessions.delete(childSessionKey);
    }
    this.dispatchContextByHelperSession.delete(dispatch.helperSessionKey);
    this.executionSessions.delete(dispatch.helperSessionKey);
  }

  private async fetchChildExecutionSummary(childSessionKey: string): Promise<{ outputSummary: string; artifactRefs: string[] }> {
    const runner = this.api.runtime?.system?.runCommandWithTimeout;
    if (typeof runner !== "function") {
      throw new Error("host runtime missing runCommandWithTimeout");
    }
    const result = await runner(
      buildGatewayCallArgs("sessions.get", { key: childSessionKey }),
      {
        timeoutMs: GATEWAY_CALL_TIMEOUT_MS,
        cwd: this.api.resolvePath("."),
      },
    );
    if (result.code !== 0) {
      throw new Error(
        `gateway sessions.get failed with exit ${String(result.code)}: ${result.stderr || result.stdout || "unknown error"}`,
      );
    }
    return extractSessionTranscriptSummary(extractJsonPayload(result.stdout));
  }

  async handleSubagentSpawned(event: Record<string, unknown>, ctx: Record<string, unknown>): Promise<void> {
    if (!(await this.ensurePeerReady())) {
      return;
    }
    const helperSessionKey =
      nonEmptyString(ctx.requesterSessionKey) ?? nonEmptyString(ctx.sessionKey);
    const childSessionKey =
      nonEmptyString(event.childSessionKey) ?? nonEmptyString(ctx.childSessionKey);
    if (!helperSessionKey || !childSessionKey) {
      return;
    }
    const dispatch = this.dispatchContextByHelperSession.get(helperSessionKey);
    if (!dispatch) {
      return;
    }
    this.executionSessions.add(childSessionKey);
    this.dispatchContextByChildSession.set(childSessionKey, dispatch);
    const payload: HostSubagentLifecycleEnvelope = {
      meta: {
        ingress_id: newId("ingress"),
        received_at: nowTimestamp(),
        causation_id: dispatch.commandId,
        correlation_id: newId("corr"),
        dedupe_window: DEDUPE_WINDOW,
      },
      command_id: dispatch.commandId,
      managed_task_ref: dispatch.managedTaskRef,
      run_ref: dispatch.runRef,
      role_kind: dispatch.roleKind,
      event: {
        spawned: {
          host_child_execution_ref: childSessionKey,
          host_child_run_ref: nonEmptyString(event.runId) ?? nonEmptyString(ctx.runId) ?? null,
          host_agent_id: dispatch.hostAgentId,
          observed_at: nowTimestamp(),
        },
      },
    };
    try {
      await this.client.postSubagentLifecycle(payload);
    } catch (error) {
      this.handleOperationFailure("subagent_spawned_post_failed", error);
    }
  }

  async handleSubagentEnded(event: Record<string, unknown>, ctx: Record<string, unknown>): Promise<void> {
    if (!(await this.ensurePeerReady())) {
      return;
    }
    const childSessionKey =
      nonEmptyString(event.targetSessionKey) ?? nonEmptyString(ctx.childSessionKey);
    const helperSessionKey =
      nonEmptyString(ctx.requesterSessionKey) ?? nonEmptyString(ctx.sessionKey);
    const dispatch =
      (childSessionKey ? this.dispatchContextByChildSession.get(childSessionKey) : undefined) ??
      (helperSessionKey ? this.dispatchContextByHelperSession.get(helperSessionKey) : undefined);
    if (!dispatch || !childSessionKey) {
      return;
    }
    let summary;
    try {
      summary = await this.fetchChildExecutionSummary(childSessionKey);
    } catch (error) {
      this.handleOperationFailure("subagent_summary_fetch_failed", error);
      summary = {
        outputSummary: `subagent finished but transcript fetch failed: ${error instanceof Error ? error.message : String(error)}`,
        artifactRefs: [],
      };
    }
    const payload: HostSubagentLifecycleEnvelope = {
      meta: {
        ingress_id: newId("ingress"),
        received_at: nowTimestamp(),
        causation_id: dispatch.commandId,
        correlation_id: newId("corr"),
        dedupe_window: DEDUPE_WINDOW,
      },
      command_id: dispatch.commandId,
      managed_task_ref: dispatch.managedTaskRef,
      run_ref: dispatch.runRef,
      role_kind: dispatch.roleKind,
      event: {
        ended: {
          host_child_execution_ref: childSessionKey,
          host_child_run_ref: nonEmptyString(event.runId) ?? nonEmptyString(ctx.runId) ?? null,
          host_agent_id: dispatch.hostAgentId,
          status: subagentStatusFromOutcome(event.outcome),
          output_summary: summary.outputSummary,
          artifact_refs: summary.artifactRefs,
          observed_at: nowTimestamp(),
        },
      },
    };
    try {
      await this.client.postSubagentLifecycle(payload);
      this.cleanupDispatchTracking(dispatch, childSessionKey);
      await this.drainHostExecution(dispatch.hostSessionId);
    } catch (error) {
      this.handleOperationFailure("subagent_ended_post_failed", error);
    }
  }

  async submitBundle(
    hostSessionId: string,
    bundle: HostSemanticBundle,
  ): Promise<{ semanticDecisionId?: string; controlActionKind?: string }> {
    if (!(await this.ensurePeerReady())) {
      throw new Error("bridge peer is not active");
    }
    const turn = this.latestTurn(hostSessionId);
    if (!turn) {
      throw new Error(`no current turn bound for session ${hostSessionId}`);
    }
    const canonicalBundle: HostSemanticBundle = {
      ...bundle,
      input_ref: turn.hostMessageRef ?? bundle.input_ref,
    };
    if (turn.hostMessageRef && bundle.input_ref !== turn.hostMessageRef) {
      this.api.logger.warn?.("bridge.peer.input_ref_canonicalized", {
        host_session_id: hostSessionId,
        provided_input_ref: bundle.input_ref,
        canonical_input_ref: turn.hostMessageRef,
      });
    }

    const semanticDecision = mapHostSemanticBundleToSemanticDecision(
      canonicalBundle,
      hostSessionId,
      turn.hostMessageRef,
    );
    const controlAction = mapHostSemanticBundleToControlAction(
      canonicalBundle,
      hostSessionId,
      turn.hostMessageRef,
    );
    if (!semanticDecision && !controlAction) {
      throw new Error("bundle produced neither semantic decision nor control action");
    }

    try {
      if (semanticDecision) {
        await this.client.postSemanticDecision(semanticDecision);
      }
      if (controlAction) {
        await this.client.postControlAction(controlAction);
      }
      this.pendingSemanticSessions.delete(hostSessionId);
      if ((semanticDecision && semanticDecision.interaction_lane !== "chat") || controlAction) {
        this.suppressAssistantSessions.add(hostSessionId);
      } else {
        this.suppressAssistantSessions.delete(hostSessionId);
      }
      await this.drainOutbound(hostSessionId);
      if (this.getBridgeStatus() !== "active") {
        return {
          semanticDecisionId: semanticDecision?.decision_id,
          controlActionKind: controlAction?.kind,
        };
      }
      await this.drainHostExecution(hostSessionId);
      return {
        semanticDecisionId: semanticDecision?.decision_id,
        controlActionKind: controlAction?.kind,
      };
    } catch (error) {
      this.handleOperationFailure("semantic_bundle_submit_failed", error);
      throw error;
    }
  }

  async drainOutbound(hostSessionId: string): Promise<void> {
    if (!(await this.ensurePeerReady())) {
      return;
    }
    while (true) {
      let outbound;
      try {
        outbound = await this.client.nextOutbound(hostSessionId);
      } catch (error) {
        this.handleOperationFailure("outbound_poll_failed", error);
        return;
      }
      if (!outbound) {
        return;
      }
      try {
        const runner = this.api.runtime?.system?.runCommandWithTimeout;
        if (typeof runner !== "function") {
          throw new Error("host runtime missing runCommandWithTimeout");
        }
        const rendered = renderPayload(outbound.payload);
        const result = await runner(
          buildGatewayCallArgs("chat.inject", {
            sessionKey: hostSessionId,
            message: rendered,
          }),
          {
            timeoutMs: GATEWAY_CALL_TIMEOUT_MS,
            cwd: this.api.resolvePath("."),
          },
        );
        if (result.code !== 0) {
          throw new Error(
            `gateway chat.inject failed with exit ${String(result.code)}: ${result.stderr || result.stdout || "unknown error"}`,
          );
        }
        const parsed = extractJsonPayload(result.stdout) as { ok?: boolean; messageId?: string };
        if (parsed.ok !== true || typeof parsed.messageId !== "string" || !parsed.messageId.trim()) {
          throw new Error(`gateway chat.inject returned invalid payload: ${result.stdout}`);
        }
        await this.client.ackOutbound(outbound.delivery_id);
      } catch (error) {
        this.handleOperationFailure("outbound_delivery_failed", error);
        return;
      }
    }
  }
}

const plugin = {
  id: ADAPTER_ID,
  register(api: OpenClawPluginApi) {
    const baseUrl = resolveBridgeBaseUrl(api);
    const runtime = new LoomOpenClawRuntime(api);
    api.logger.info?.("loom-openclaw registered", { baseUrl });

    api.on("message_received", async (event, ctx) => {
      const canonical =
        nonEmptyString((ctx as Record<string, unknown>).sessionKey) ??
        nonEmptyString((event as Record<string, unknown>).sessionKey) ??
        resolveSessionFromMessageContext(
          api,
          ctx as Record<string, unknown>,
          event as Record<string, unknown>,
        ) ??
        resolveCanonicalSession(
          ctx as Record<string, unknown>,
          event as Record<string, unknown>,
        );
      if (!canonical) {
        return;
      }
      const text = extractInboundText(event as Record<string, unknown>);
      if (!text.trim()) {
        return;
      }
      if (runtime.isExecutionSession(canonical)) {
        return;
      }
      if (isInternalExecutionText(text)) {
        return;
      }
      await runtime.ingestCurrentTurn(canonical, extractHostMessageRef(event as Record<string, unknown>), text);
    });

    api.on("before_agent_start", async (_event, ctx) => {
      const canonical = resolveCanonicalSession(ctx as Record<string, unknown>);
      if (!canonical || runtime.isExecutionSession(canonical)) {
        return;
      }
      await runtime.syncCapabilities(canonical);
    });

    api.on("before_prompt_build", (_event, ctx) => {
      const canonical = resolveCanonicalSession(ctx as Record<string, unknown>);
      if (
        !canonical ||
        runtime.isExecutionSession(canonical) ||
        runtime.getBridgeStatus() !== "active" ||
        !runtime.needsSemantic(canonical)
      ) {
        return;
      }
      return { prependContext: buildGovernancePrompt() };
    });

    api.on("message_sending", (_event, ctx) => {
      const canonical = resolveCanonicalSession(ctx as Record<string, unknown>);
      if (!canonical) {
        return;
      }
      if (runtime.isExecutionSession(canonical)) {
        return { cancel: true };
      }
      if (runtime.needsSemantic(canonical) || runtime.shouldSuppressAssistant(canonical)) {
        return { cancel: true };
      }
    });

    api.on("before_message_write", (event, ctx) => {
      const canonical = resolveCanonicalSession(ctx as Record<string, unknown>);
      const message = (event as Record<string, unknown>).message;
      if (canonical && runtime.isExecutionSession(canonical)) {
        return { block: true };
      }
      if (containsInternalToolCall(message)) {
        return { block: true };
      }
      if (isInternalExecutionText(extractTextContent((message as Record<string, unknown> | undefined)?.content))) {
        return { block: true };
      }
      if (
        canonical &&
        (runtime.needsSemantic(canonical) || runtime.shouldSuppressAssistant(canonical)) &&
        isAssistantMessage(message)
      ) {
        return { block: true };
      }
    });

    api.on("tool_result_persist", (event, ctx) => {
      const message = (event as Record<string, unknown>).message;
      const toolName = (ctx as Record<string, unknown>).toolName;
      if (toolName !== TOOL_NAME || !message || typeof message !== "object") {
        return;
      }
      return {
        message: {
          ...(message as Record<string, unknown>),
          transcriptVisibility: "internal",
        },
      };
    });

    api.on("subagent_spawned", async (event, ctx) => {
      await runtime.handleSubagentSpawned(
        event as Record<string, unknown>,
        ctx as Record<string, unknown>,
      );
    });

    api.on("subagent_ended", async (event, ctx) => {
      await runtime.handleSubagentEnded(
        event as Record<string, unknown>,
        ctx as Record<string, unknown>,
      );
    });

    api.registerTool(
      (toolCtx) => ({
        name: TOOL_NAME,
        description: "Emit one structured HostSemanticBundle for the current host turn.",
        parameters: {
          type: "object",
          additionalProperties: false,
          properties: {
            schema_version: {
              type: "object",
              additionalProperties: false,
              properties: {
                major: { type: "number" },
                minor: { type: "number" },
              },
              required: ["major", "minor"],
            },
            input_ref: { type: "string" },
            source_model_ref: { type: "string" },
            issued_at: { type: "string" },
            decisions: {
              type: "array",
              minItems: 1,
              items: {
                oneOf: hostSemanticDecisionSchemas(),
              },
              anyOf: [
                {
                  contains: {
                    type: "object",
                    properties: {
                      decision_kind: { const: "interaction_lane" },
                      payload: {
                        type: "object",
                        properties: {
                          interaction_lane: { enum: [...INTERACTION_LANES] },
                        },
                        required: ["interaction_lane"],
                      },
                    },
                    required: ["decision_kind", "payload"],
                  },
                },
                {
                  contains: {
                    type: "object",
                    properties: {
                      decision_kind: { const: "control_action" },
                      payload: {
                        type: "object",
                        properties: {
                          action_kind: { enum: [...CONTROL_ACTIONS] },
                        },
                        required: ["action_kind"],
                      },
                    },
                    required: ["decision_kind", "payload"],
                  },
                },
              ],
            },
            rationale_summary: { type: "string" },
          },
          required: ["schema_version", "input_ref", "source_model_ref", "issued_at", "decisions"],
        },
        async execute(_toolCallId: string, params: unknown) {
          const ctxSession = runtime.latestTurn(
            resolveCanonicalSession(toolCtx as Record<string, unknown>) ?? "",
          );
          void ctxSession;
          const bundle = params as HostSemanticBundle;
          if (bundle.schema_version?.major !== 0) {
            throw new Error(`unsupported schema major version: ${bundle.schema_version?.major}`);
          }
          const hostSessionId = resolveCanonicalSession(toolCtx as Record<string, unknown>);
          if (!hostSessionId) {
            throw new Error("no current host session is available for semantic submission");
          }
          const result = await runtime.submitBundle(hostSessionId, bundle);
          return { ok: true, ...result };
        },
      }),
      { name: TOOL_NAME },
    );

    api.registerService({
      id: SERVICE_ID,
      start: async () => {
        await runtime.startPeer();
      },
      stop: async () => {
        runtime.stopPeer();
      },
    });

    return {
      baseUrl,
      runtime: {
        getBridgeStatus: () => runtime.getBridgeStatus(),
      },
      helpers: {
        mapHostSemanticBundleToSemanticDecision,
        mapHostSemanticBundleToControlAction,
        renderPayload,
      },
    };
  },
};

export default plugin;
export { createLoomBridgeClient, mapHostSemanticBundleToControlAction, mapHostSemanticBundleToSemanticDecision, renderPayload };

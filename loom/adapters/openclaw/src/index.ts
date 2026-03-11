import { createHash, randomUUID } from "node:crypto";
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import type { OpenClawPluginApi, PluginCommandContext } from "openclaw/plugin-sdk";

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
  CurrentControlSurfaceProjection,
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
    baseUrl?: string;
    runtimeRoot?: string;
  };
};

const DEFAULT_BRIDGE_URL = "http://127.0.0.1:6417";
const ADAPTER_ID = "loom-openclaw";
const TOOL_NAME = "loom_emit_host_semantic_bundle";
const SERVICE_ID = "loom-openclaw-peer";
const RUNTIME_ROOT_RELATIVE_PATH = "runtime";
const BOOTSTRAP_TICKET_RUNTIME_SUBPATH = "loom/bootstrap/openclaw/bootstrap-ticket.json";
const COMMAND_PROBE_RUNTIME_SUBDIR = "loom/host-bridges/openclaw/command-probe";
const COMMAND_PROBE_DIRNAME = "command-probe";
const COMMAND_PROBE_LATEST_FILENAME = "latest.json";
const COMMAND_PROBE_EVENTS_FILENAME = "events.jsonl";
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

type ControlSurfaceProbeSnapshot = {
  surfaceType: "start_card" | "boundary_card" | "approval_request";
  managedTaskRef: string;
  allowedActions: string[];
  decisionTokenDigest: string;
  cachedAt: string;
  deliveryId: string;
};

type ProbeEventKind = "message_received" | "command_invoked";

type ProbeEvent = {
  sequence: number;
  kind: ProbeEventKind;
  text: string;
  hostSessionId?: string;
  hostMessageRef?: string;
  recordedAt: string;
};

type CommandResolutionAttempt = {
  peerId: string;
  via: "raw" | "resolveAgentRoute" | "buildAgentSessionKey";
  sessionKey?: string;
  error?: string;
};

type CommandSessionProbe = {
  channelId?: string;
  accountId?: string;
  conversationCandidates: string[];
  attempts: CommandResolutionAttempt[];
  canonical?: string;
};

type LoomCommandVerb =
  | "help"
  | "probe"
  | "approve"
  | "cancel"
  | "modify"
  | "keep"
  | "replace"
  | "reject";

type ParsedLoomCommand =
  | { verb: "help" | "probe" | "approve" | "cancel" | "keep" | "replace" | "reject" }
  | { verb: "modify"; payloadText: string };

type ProbeValueSummary =
  | { kind: "undefined" }
  | { kind: "null" }
  | { kind: "boolean"; value: boolean }
  | { kind: "number"; value: number }
  | { kind: "string"; value: string; length: number; truncated: boolean }
  | { kind: "array"; length: number; itemKinds: string[] }
  | { kind: "object"; keyCount: number; keys: string[]; redacted?: boolean }
  | { kind: "function" }
  | { kind: "other"; type: string };

type CommandContextShape = {
  keys: string[];
  fields: Record<string, ProbeValueSummary>;
};

type LatestTurnProbeSnapshot = {
  hostSessionId: string;
  hostMessageRef?: string;
  text: string;
  ingressId: string;
  correlationId: string;
  receivedAt: string;
  textMatchesCommand: boolean;
};

type CommandInvocationProbe = {
  recordedAt: string;
  commandEventSequence: number;
  commandBody: string;
  args?: string;
  authorized: boolean;
  resolvedHostSessionId?: string;
  conversationCandidates: string[];
  resolutionAttempts: CommandResolutionAttempt[];
  commandContext: CommandContextShape;
  latestTurnAtInvoke?: LatestTurnProbeSnapshot;
  latestControlSurfaceAtInvoke?: ControlSurfaceProbeSnapshot;
};

type MatchingMessageOrder = "before_command" | "after_command" | "both_sides" | "not_observed";

type CommandProbeProjection = {
  updatedAt: string;
  recentEvents: ProbeEvent[];
  lastCommand?: CommandInvocationProbe & {
    messageReceivedObserved: boolean;
    matchingMessageOrder: MatchingMessageOrder;
    matchingMessageEventSequences: number[];
  };
};

function commandKey(hostSessionId: string, commandBody: string): string {
  return `${hostSessionId}\n${commandBody}`;
}

function parseLoomCommand(ctx: PluginCommandContext): ParsedLoomCommand {
  const rawArgs = nonEmptyString(ctx.args)?.trim() ?? "";
  if (!rawArgs) {
    return { verb: "help" };
  }
  const [verbToken] = rawArgs.split(/\s+/, 1);
  switch (verbToken) {
    case "help":
      return { verb: "help" };
    case "probe":
      return { verb: "probe" };
    case "approve":
      return { verb: "approve" };
    case "cancel":
      return { verb: "cancel" };
    case "keep":
      return { verb: "keep" };
    case "replace":
      return { verb: "replace" };
    case "reject":
      return { verb: "reject" };
    case "modify": {
      const payloadText = rawArgs.slice("modify".length).trim();
      return { verb: "modify", payloadText };
    }
    default:
      throw new Error(
        `unknown /loom command: ${verbToken}. Supported commands: approve, cancel, modify, keep, replace, reject, probe.`,
      );
  }
}

function displayCommandForAction(action: CurrentControlSurfaceProjection["allowed_actions"][number]): string {
  switch (action) {
    case "approve_start":
    case "approve_request":
      return "/loom approve";
    case "modify_candidate":
      return "/loom modify <summary or JSON>";
    case "cancel_candidate":
      return "/loom cancel";
    case "keep_current_task":
      return "/loom keep";
    case "replace_active":
      return "/loom replace";
    case "reject_request":
      return "/loom reject";
    default:
      return `/loom ${action}`;
  }
}

function availableCommands(surface: CurrentControlSurfaceProjection): string[] {
  return [...new Set(surface.allowed_actions.map(displayCommandForAction))];
}

function buildControlSurfaceHelpText(surface: CurrentControlSurfaceProjection | null): string {
  if (!surface) {
    return [
      "No open Loom control surface for this session.",
      "Use `/loom probe` if you need transport diagnostics.",
    ].join("\n");
  }
  return [
    `Current Loom control surface: ${surface.surface_type}`,
    `managed_task_ref: ${surface.managed_task_ref}`,
    `allowed_actions: ${surface.allowed_actions.join(", ")}`,
    "Commands:",
    ...availableCommands(surface).map((command) => `- ${command}`),
  ].join("\n");
}

function resolveSlashActionKind(
  command: ParsedLoomCommand,
  surface: CurrentControlSurfaceProjection,
): CurrentControlSurfaceProjection["allowed_actions"][number] {
  const allowed = new Set(surface.allowed_actions);
  switch (command.verb) {
    case "approve":
      if (allowed.has("approve_start")) {
        return "approve_start";
      }
      if (allowed.has("approve_request")) {
        return "approve_request";
      }
      break;
    case "cancel":
      if (allowed.has("cancel_candidate")) {
        return "cancel_candidate";
      }
      break;
    case "modify":
      if (allowed.has("modify_candidate")) {
        return "modify_candidate";
      }
      break;
    case "keep":
      if (allowed.has("keep_current_task")) {
        return "keep_current_task";
      }
      break;
    case "replace":
      if (allowed.has("replace_active")) {
        return "replace_active";
      }
      break;
    case "reject":
      if (allowed.has("reject_request")) {
        return "reject_request";
      }
      break;
    default:
      break;
  }
  throw new Error(
    `/loom ${command.verb} is not allowed for the current ${surface.surface_type}. Allowed commands: ${availableCommands(surface).join(", ") || "(none)"}`,
  );
}

function parseModifyPayload(payloadText: string): NonNullable<
  Extract<
    HostSemanticBundle["decisions"][number],
    { decision_kind: "control_action" }
  >["payload"]["payload"]
> {
  if (!payloadText.trim()) {
    throw new Error(
      "/loom modify requires a summary string or a JSON payload like {\"summary\":\"...\"}.",
    );
  }
  if (!payloadText.trim().startsWith("{")) {
    return {
      summary: payloadText.trim(),
      rationale: "slash command /loom modify",
    };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(payloadText);
  } catch (error) {
    throw new Error(
      `invalid /loom modify JSON payload: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("/loom modify JSON payload must be an object.");
  }
  const value = parsed as Record<string, unknown>;
  const requirementItems = Array.isArray(value.requirement_items)
    ? value.requirement_items.flatMap((item) => {
        if (typeof item === "string" && item.trim()) {
          return [{ text: item.trim(), origin: "task_change" as const }];
        }
        if (
          item &&
          typeof item === "object" &&
          typeof (item as Record<string, unknown>).text === "string"
        ) {
          const text = (item as Record<string, string>).text.trim();
          if (!text) {
            return [];
          }
          return [
            {
              text,
              origin:
                typeof (item as Record<string, unknown>).origin === "string"
                  ? (item as Record<string, string>).origin
                  : "task_change",
            },
          ];
        }
        return [];
      })
    : [];
  return {
    title: nonEmptyString(value.title),
    summary: nonEmptyString(value.summary),
    expected_outcome: nonEmptyString(value.expected_outcome),
    requirement_items: requirementItems,
    allowed_roots: Array.isArray(value.allowed_roots)
      ? value.allowed_roots.flatMap((item) => (typeof item === "string" && item.trim() ? [item.trim()] : []))
      : [],
    secret_classes: Array.isArray(value.secret_classes)
      ? value.secret_classes.flatMap((item) => (typeof item === "string" && item.trim() ? [item.trim()] : []))
      : [],
    workspace_ref: nonEmptyString(value.workspace_ref),
    repo_ref: nonEmptyString(value.repo_ref),
    rationale: nonEmptyString(value.rationale) ?? "slash command /loom modify",
  };
}

function buildControlActionBundle(
  command: ParsedLoomCommand,
  surface: CurrentControlSurfaceProjection,
  commandBody: string,
): HostSemanticBundle {
  const actionKind = resolveSlashActionKind(command, surface);
  return {
    schema_version: { major: 0, minor: 1 },
    input_ref: commandBody,
    source_model_ref: "loom-slash-command",
    issued_at: nowTimestamp(),
    decisions: [
      {
        decision_kind: "control_action",
        decision_source: "user_control_action",
        confidence: 0.99,
        rationale: `explicit ${actionKind} from /loom command`,
        payload: {
          action_kind: actionKind,
          managed_task_ref: surface.managed_task_ref,
          decision_token: surface.decision_token,
          payload: command.verb === "modify" ? parseModifyPayload(command.payloadText) : undefined,
        },
      },
    ],
    rationale_summary: `slash command ${commandBody}`,
  };
}

function resolveBridgeBaseUrl(api: OpenClawPluginApi): string {
  const configured = nonEmptyString(api.getConfig?.<string>("bridge.baseUrl"));
  if (configured) return configured;
  const pluginConfig = api.pluginConfig as LoomOpenClawConfig | undefined;
  return nonEmptyString(pluginConfig?.bridge?.baseUrl) ?? DEFAULT_BRIDGE_URL;
}

function resolveBridgeRuntimeRoot(api: OpenClawPluginApi): string {
  const configured =
    nonEmptyString(api.getConfig?.<string>("bridge.runtimeRoot")) ??
    nonEmptyString((api.pluginConfig as LoomOpenClawConfig | undefined)?.bridge?.runtimeRoot);
  if (configured) {
    if (!isAbsolute(configured)) {
      throw new Error(`bridge.runtimeRoot must be an absolute path: ${configured}`);
    }
    return configured;
  }

  const legacyRuntimeRoot = api.resolvePath(RUNTIME_ROOT_RELATIVE_PATH);
  if (existsSync(legacyRuntimeRoot)) {
    api.logger.warn?.("bridge.runtime_root.legacy_relative_fallback", {
      runtime_root: legacyRuntimeRoot,
      config_key: "bridge.runtimeRoot",
    });
    return legacyRuntimeRoot;
  }

  throw new Error(
    `bridge.runtimeRoot is required; legacy fallback not found at ${legacyRuntimeRoot}`,
  );
}

function resolveBootstrapTicketPath(api: OpenClawPluginApi): string {
  return join(resolveBridgeRuntimeRoot(api), BOOTSTRAP_TICKET_RUNTIME_SUBPATH);
}

function resolveCommandProbeRuntimeDir(api: OpenClawPluginApi): string {
  return join(resolveBridgeRuntimeRoot(api), COMMAND_PROBE_RUNTIME_SUBDIR);
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

function hashText(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 12);
}

function truncateText(value: string, maxLength = 160): { value: string; truncated: boolean } {
  if (value.length <= maxLength) {
    return { value, truncated: false };
  }
  return {
    value: `${value.slice(0, Math.max(0, maxLength - 3))}...`,
    truncated: true,
  };
}

function uniqueNonEmptyStrings(values: Array<string | undefined>): string[] {
  return [...new Set(values.filter((value): value is string => typeof value === "string" && value.length > 0))];
}

function summarizeProbeValue(fieldName: string, value: unknown): ProbeValueSummary {
  if (value === undefined) {
    return { kind: "undefined" };
  }
  if (value === null) {
    return { kind: "null" };
  }
  if (typeof value === "boolean") {
    return { kind: "boolean", value };
  }
  if (typeof value === "number") {
    return { kind: "number", value };
  }
  if (typeof value === "string") {
    const truncated = truncateText(value);
    return {
      kind: "string",
      value: truncated.value,
      length: value.length,
      truncated: truncated.truncated,
    };
  }
  if (Array.isArray(value)) {
    return {
      kind: "array",
      length: value.length,
      itemKinds: [...new Set(value.map((item) => (item === null ? "null" : Array.isArray(item) ? "array" : typeof item)))].sort(),
    };
  }
  if (typeof value === "function") {
    return { kind: "function" };
  }
  if (typeof value === "object") {
    const keys = Object.keys(value as Record<string, unknown>).sort();
    return {
      kind: "object",
      keyCount: keys.length,
      keys: keys.slice(0, 20),
      redacted: fieldName === "config",
    };
  }
  return { kind: "other", type: typeof value };
}

function summarizeCommandContext(ctx: PluginCommandContext): CommandContextShape {
  const fields: Record<string, ProbeValueSummary> = {};
  const keys = Object.keys(ctx as Record<string, unknown>).sort();
  for (const key of keys) {
    fields[key] = summarizeProbeValue(key, (ctx as Record<string, unknown>)[key]);
  }
  return { keys, fields };
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
  const ticketPath = resolveBootstrapTicketPath(api);
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

function resolveCommandSessionProbe(
  api: OpenClawPluginApi,
  ctx: PluginCommandContext,
): CommandSessionProbe {
  const channelId = nonEmptyString(ctx.channelId) ?? nonEmptyString(ctx.channel);
  const accountId = nonEmptyString(ctx.accountId);
  const conversationCandidates = uniqueNonEmptyStrings([
    nonEmptyString(ctx.to),
    nonEmptyString(ctx.from),
    nonEmptyString(ctx.senderId),
  ]);
  const attempts: CommandResolutionAttempt[] = [];
  let canonical = uniqueNonEmptyStrings([nonEmptyString((ctx as Record<string, unknown>).sessionKey)])[0];

  for (const peerId of conversationCandidates) {
    if (peerId.startsWith("agent:")) {
      attempts.push({ peerId, via: "raw", sessionKey: peerId });
      canonical ??= peerId;
      continue;
    }
    if (!channelId) {
      continue;
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
          accountId: accountId ?? null,
          peer: { kind: "direct", id: peerId },
        }) as Record<string, unknown> | undefined;
        const sessionKey = nonEmptyString(route?.sessionKey);
        attempts.push({ peerId, via: "resolveAgentRoute", sessionKey });
        if (!canonical && sessionKey) {
          canonical = sessionKey;
        }
      } catch (error) {
        attempts.push({
          peerId,
          via: "resolveAgentRoute",
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    const buildAgentSessionKey = helpers?.buildAgentSessionKey;
    if (typeof buildAgentSessionKey === "function") {
      try {
        const sessionKey = nonEmptyString(
          buildAgentSessionKey({
            agentId: resolveDefaultAgentId(api),
            channel: channelId,
            accountId: accountId ?? null,
            peer: { kind: "direct", id: peerId },
          }),
        );
        attempts.push({ peerId, via: "buildAgentSessionKey", sessionKey });
        if (!canonical && sessionKey) {
          canonical = sessionKey;
        }
      } catch (error) {
        attempts.push({
          peerId,
          via: "buildAgentSessionKey",
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
  }

  return {
    channelId,
    accountId,
    conversationCandidates,
    attempts,
    canonical,
  };
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

function toControlSurfaceProbeSnapshot(
  deliveryId: string,
  payload: Parameters<typeof renderPayload>[0],
): ControlSurfaceProbeSnapshot | null {
  switch (payload.type) {
    case "start_card":
      return {
        surfaceType: "start_card",
        managedTaskRef: payload.data.managed_task_ref,
        allowedActions: payload.data.allowed_actions,
        decisionTokenDigest: hashText(payload.data.decision_token),
        cachedAt: nowTimestamp(),
        deliveryId,
      };
    case "boundary_card":
      return {
        surfaceType: "boundary_card",
        managedTaskRef: payload.data.managed_task_ref,
        allowedActions: payload.data.allowed_actions,
        decisionTokenDigest: hashText(payload.data.decision_token),
        cachedAt: nowTimestamp(),
        deliveryId,
      };
    case "approval_request":
      return {
        surfaceType: "approval_request",
        managedTaskRef: payload.data.managed_task_ref,
        allowedActions: payload.data.allowed_actions,
        decisionTokenDigest: hashText(payload.data.decision_token),
        cachedAt: nowTimestamp(),
        deliveryId,
      };
    default:
      return null;
  }
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

function absoluteHostPath(value: unknown): string | undefined {
  const text = nonEmptyString(value);
  return text && isAbsolute(text) ? text : undefined;
}

function readAgentDefaults(config: unknown): Record<string, unknown> | undefined {
  if (
    !config ||
    typeof config !== "object" ||
    !("agents" in config) ||
    !(config.agents && typeof config.agents === "object") ||
    !("defaults" in (config.agents as Record<string, unknown>)) ||
    !((config.agents as Record<string, unknown>).defaults &&
      typeof (config.agents as Record<string, unknown>).defaults === "object")
  ) {
    return undefined;
  }
  return (config.agents as Record<string, unknown>).defaults as Record<string, unknown>;
}

function resolveHostWorkspaceRoot(
  api: OpenClawPluginApi,
  ctx?: Record<string, unknown>,
): string | undefined {
  const config = (api as { config?: Record<string, unknown> }).config;
  const agents = readAgentEntries(config);
  const currentAgent = selectCurrentAgent(agents);
  return (
    absoluteHostPath(ctx?.workspaceDir) ??
    absoluteHostPath(currentAgent?.workspace) ??
    absoluteHostPath(readAgentDefaults(config)?.workspace)
  );
}

function resolveHostRepoRef(_api: OpenClawPluginApi, ctx?: Record<string, unknown>): string | undefined {
  return nonEmptyString(ctx?.repoRef);
}

function buildCurrentTurn(
  hostSessionId: string,
  hostMessageRef: string | undefined,
  text: string,
  workspaceRoot?: string,
  repoRef?: string,
): CurrentTurnEnvelope {
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
    workspace_ref: workspaceRoot ?? null,
    repo_ref: repoRef ?? null,
  };
}

function buildCapabilitySnapshot(
  hostSessionId: string,
  api: OpenClawPluginApi,
  workspaceRoot?: string,
): HostCapabilitySnapshot {
  const config = (api as { config?: Record<string, unknown> }).config;
  const agents = readAgentEntries(config);
  const currentAgent = selectCurrentAgent(agents);
  const availableAgents = currentAgent ? computeSpawnableChildAgents(config, currentAgent, agents) : [];
  return {
    capability_snapshot_ref: newId("cap"),
    host_session_id: hostSessionId,
    allowed_tools: [TOOL_NAME],
    readable_roots: workspaceRoot ? [workspaceRoot] : [],
    writable_roots: workspaceRoot ? [workspaceRoot] : [],
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
  private readonly controlSurfaceBySession = new Map<string, ControlSurfaceProbeSnapshot>();
  private readonly pendingSemanticSessions = new Set<string>();
  private readonly suppressAssistantSessions = new Set<string>();
  private readonly capabilityDigestBySession = new Map<string, string>();
  private readonly executionSessions = new Set<string>();
  private readonly dispatchContextByHelperSession = new Map<string, CommandDispatchContext>();
  private readonly dispatchContextByChildSession = new Map<string, CommandDispatchContext>();
  private readonly drainingExecutionSessions = new Set<string>();
  private readonly probeEvents: ProbeEvent[] = [];
  private readonly handledLoomCommands = new Map<string, number>();
  private probeSequence = 0;
  private lastCommandProbe?: CommandInvocationProbe;
  private commandProbeOutputRoot?: string;

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
    this.controlSurfaceBySession.clear();
    this.executionSessions.clear();
    this.dispatchContextByHelperSession.clear();
    this.dispatchContextByChildSession.clear();
    this.drainingExecutionSessions.clear();
  }

  setCommandProbeOutputRoot(outputRoot: string | undefined): void {
    this.commandProbeOutputRoot = nonEmptyString(outputRoot);
    if (this.commandProbeOutputRoot) {
      this.api.logger.info?.("loom.command.probe_output_root", {
        output_root: this.commandProbeOutputRoot,
      });
    }
  }

  private commandProbeDirPath(): string {
    return this.commandProbeOutputRoot
      ? join(this.commandProbeOutputRoot, COMMAND_PROBE_DIRNAME)
      : resolveCommandProbeRuntimeDir(this.api);
  }

  private commandProbeLatestPath(): string {
    return join(this.commandProbeDirPath(), COMMAND_PROBE_LATEST_FILENAME);
  }

  private commandProbeEventsPath(): string {
    return join(this.commandProbeDirPath(), COMMAND_PROBE_EVENTS_FILENAME);
  }

  private matchingMessageEventsForCommand(command: CommandInvocationProbe): ProbeEvent[] {
    return this.probeEvents.filter(
      (event) =>
        event.kind === "message_received" &&
        event.text === command.commandBody &&
        (!command.resolvedHostSessionId || event.hostSessionId === command.resolvedHostSessionId),
    );
  }

  private resolveMatchingMessageOrder(
    commandEventSequence: number,
    matchingEvents: ProbeEvent[],
  ): MatchingMessageOrder {
    if (matchingEvents.length === 0) {
      return "not_observed";
    }
    const hasBefore = matchingEvents.some((event) => event.sequence < commandEventSequence);
    const hasAfter = matchingEvents.some((event) => event.sequence > commandEventSequence);
    if (hasBefore && hasAfter) {
      return "both_sides";
    }
    return hasBefore ? "before_command" : "after_command";
  }

  private buildCommandProbeProjection(): CommandProbeProjection {
    const recentEvents = [...this.probeEvents];
    return {
      updatedAt: nowTimestamp(),
      recentEvents,
      lastCommand: this.lastCommandProbe
        ? (() => {
            const matchingMessageEvents = this.matchingMessageEventsForCommand(this.lastCommandProbe);
            return {
              ...this.lastCommandProbe,
              messageReceivedObserved: matchingMessageEvents.length > 0,
              matchingMessageOrder: this.resolveMatchingMessageOrder(
                this.lastCommandProbe.commandEventSequence,
                matchingMessageEvents,
              ),
              matchingMessageEventSequences: matchingMessageEvents.map((event) => event.sequence),
            };
          })()
        : undefined,
    };
  }

  private persistCommandProbeProjection(): void {
    try {
      mkdirSync(this.commandProbeDirPath(), { recursive: true });
      writeFileSync(
        this.commandProbeLatestPath(),
        `${JSON.stringify(this.buildCommandProbeProjection(), null, 2)}\n`,
      );
    } catch (error) {
      this.api.logger.warn?.("loom.command.probe_projection_write_failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private appendProbeEventProjection(event: ProbeEvent): void {
    try {
      mkdirSync(this.commandProbeDirPath(), { recursive: true });
      appendFileSync(this.commandProbeEventsPath(), `${JSON.stringify(event)}\n`);
    } catch (error) {
      this.api.logger.warn?.("loom.command.probe_event_write_failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private rememberCommandProbe(
    ctx: PluginCommandContext,
    probe: CommandSessionProbe,
    commandEventSequence: number,
    latestTurn?: TurnBinding,
    latestControlSurface?: ControlSurfaceProbeSnapshot,
  ): void {
    this.lastCommandProbe = {
      recordedAt: nowTimestamp(),
      commandEventSequence,
      commandBody: ctx.commandBody,
      args: ctx.args,
      authorized: ctx.isAuthorizedSender,
      resolvedHostSessionId: probe.canonical,
      conversationCandidates: probe.conversationCandidates,
      resolutionAttempts: probe.attempts,
      commandContext: summarizeCommandContext(ctx),
      latestTurnAtInvoke: latestTurn
        ? {
            hostSessionId: latestTurn.hostSessionId,
            hostMessageRef: latestTurn.hostMessageRef,
            text: latestTurn.text,
            ingressId: latestTurn.ingressId,
            correlationId: latestTurn.correlationId,
            receivedAt: latestTurn.receivedAt,
            textMatchesCommand: latestTurn.text === ctx.commandBody,
          }
        : undefined,
      latestControlSurfaceAtInvoke: latestControlSurface,
    };
    this.persistCommandProbeProjection();
  }

  private recordProbeEvent(kind: ProbeEventKind, text: string, extra?: Partial<ProbeEvent>): ProbeEvent {
    const event: ProbeEvent = {
      sequence: (this.probeSequence += 1),
      kind,
      text,
      hostSessionId: extra?.hostSessionId,
      hostMessageRef: extra?.hostMessageRef,
      recordedAt: nowTimestamp(),
    };
    this.probeEvents.push(event);
    while (this.probeEvents.length > 20) {
      this.probeEvents.shift();
    }
    this.api.logger.info?.("loom.command.probe_event", event);
    this.appendProbeEventProjection(event);
    this.persistCommandProbeProjection();
    return event;
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

  rememberHandledLoomCommand(hostSessionId: string, commandBody: string): void {
    const now = Date.now();
    this.handledLoomCommands.set(commandKey(hostSessionId, commandBody), now);
    for (const [key, recordedAt] of this.handledLoomCommands.entries()) {
      if (now - recordedAt > 60_000) {
        this.handledLoomCommands.delete(key);
      }
    }
    this.pendingSemanticSessions.delete(hostSessionId);
  }

  wasRecentlyHandledLoomCommand(hostSessionId: string, text: string): boolean {
    const recordedAt = this.handledLoomCommands.get(commandKey(hostSessionId, text));
    if (!recordedAt) {
      return false;
    }
    if (Date.now() - recordedAt > 60_000) {
      this.handledLoomCommands.delete(commandKey(hostSessionId, text));
      return false;
    }
    return true;
  }

  latestTurn(hostSessionId: string): TurnBinding | undefined {
    return this.turnsBySession.get(hostSessionId);
  }

  latestControlSurface(hostSessionId: string): ControlSurfaceProbeSnapshot | undefined {
    return this.controlSurfaceBySession.get(hostSessionId);
  }

  async readCurrentControlSurface(
    hostSessionId: string,
  ): Promise<CurrentControlSurfaceProjection | null> {
    if (!(await this.ensurePeerReady())) {
      throw new Error("bridge peer is not active");
    }
    try {
      return await this.client.readCurrentControlSurface(hostSessionId);
    } catch (error) {
      this.handleOperationFailure("current_control_surface_read_failed", error);
      throw error;
    }
  }

  buildCommandProbeReport(ctx: PluginCommandContext, probe: CommandSessionProbe): string {
    this.recordCommandInvocation(ctx, probe);
    return this.renderRecordedCommandProbeReport(ctx, probe);
  }

  renderRecordedCommandProbeReport(ctx: PluginCommandContext, probe: CommandSessionProbe): string {
    const latestTurn = probe.canonical ? this.latestTurn(probe.canonical) : undefined;
    const latestControlSurface = probe.canonical ? this.latestControlSurface(probe.canonical) : undefined;
    return this.formatLastCommandProbeReport(
      ctx,
      probe,
      latestTurn,
      latestControlSurface,
      this.lastCommandProbe?.commandEventSequence ?? 0,
    );
  }

  recordCommandInvocation(ctx: PluginCommandContext, probe: CommandSessionProbe): void {
    const latestTurn = probe.canonical ? this.latestTurn(probe.canonical) : undefined;
    const latestControlSurface = probe.canonical ? this.latestControlSurface(probe.canonical) : undefined;
    const commandEvent = this.recordProbeEvent("command_invoked", ctx.commandBody, {
      hostSessionId: probe.canonical,
    });
    this.rememberCommandProbe(
      ctx,
      probe,
      commandEvent.sequence,
      latestTurn,
      latestControlSurface,
    );
  }

  private formatLastCommandProbeReport(
    ctx: PluginCommandContext,
    probe: CommandSessionProbe,
    latestTurn: TurnBinding | undefined,
    latestControlSurface: ControlSurfaceProbeSnapshot | undefined,
    commandEventSequence: number,
  ): string {
    const matchingMessageEvents = this.lastCommandProbe
      ? this.matchingMessageEventsForCommand(this.lastCommandProbe)
      : [];
    const matchingMessageOrder = this.lastCommandProbe
      ? this.resolveMatchingMessageOrder(commandEventSequence, matchingMessageEvents)
      : "not_observed";

    return [
      "Loom command probe",
      `commandBody: ${ctx.commandBody}`,
      `args: ${ctx.args ?? "n/a"}`,
      `authorized: ${String(ctx.isAuthorizedSender)}`,
      `channel: ${ctx.channel}`,
      `channelId: ${ctx.channelId ?? "n/a"}`,
      `from: ${ctx.from ?? "n/a"}`,
      `to: ${ctx.to ?? "n/a"}`,
      `accountId: ${ctx.accountId ?? "n/a"}`,
      `messageThreadId: ${typeof ctx.messageThreadId === "number" ? String(ctx.messageThreadId) : "n/a"}`,
      `resolvedHostSessionId: ${probe.canonical ?? "unresolved"}`,
      `conversationCandidates: ${probe.conversationCandidates.join(", ") || "n/a"}`,
      `resolutionAttempts: ${probe.attempts.map((attempt) => `${attempt.peerId}:${attempt.via}:${attempt.sessionKey ?? `error=${attempt.error ?? "none"}`}`).join(" | ") || "n/a"}`,
      `latestTurnTextMatchesCommand: ${String(latestTurn?.text === ctx.commandBody)}`,
      `latestTurnHostMessageRef: ${latestTurn?.hostMessageRef ?? "n/a"}`,
      `latestTurnText: ${latestTurn?.text ?? "n/a"}`,
      `matchingMessageReceivedObserved: ${String(matchingMessageEvents.length > 0)}`,
      `matchingMessageOrder: ${matchingMessageOrder}`,
      `matchingMessageSequences: ${matchingMessageEvents.map((event) => event.sequence).join(", ") || "n/a"}`,
      `commandContextKeys: ${Object.keys(ctx as Record<string, unknown>).sort().join(", ") || "n/a"}`,
      latestControlSurface
        ? `cachedControlSurface: ${latestControlSurface.surfaceType} task=${latestControlSurface.managedTaskRef} actions=${latestControlSurface.allowedActions.join(",")} tokenDigest=${latestControlSurface.decisionTokenDigest} cachedAt=${latestControlSurface.cachedAt}`
        : "cachedControlSurface: none",
      "recentProbeEvents:",
      ...(this.probeEvents.length > 0
        ? this.probeEvents.map(
            (event) =>
              `${event.sequence}. ${event.kind} session=${event.hostSessionId ?? "n/a"} messageRef=${event.hostMessageRef ?? "n/a"} text=${event.text}`,
          )
        : ["none"]),
    ].join("\n");
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

  private gatewayCommandCwd(ctx?: Record<string, unknown>): string | undefined {
    return resolveHostWorkspaceRoot(this.api, ctx);
  }

  async ingestCurrentTurn(
    hostSessionId: string,
    hostMessageRef: string | undefined,
    text: string,
    ctx?: Record<string, unknown>,
  ): Promise<void> {
    if (this.isExecutionSession(hostSessionId)) {
      return;
    }
    this.recordProbeEvent("message_received", text, {
      hostSessionId,
      hostMessageRef,
    });
    if (this.wasRecentlyHandledLoomCommand(hostSessionId, text)) {
      return;
    }
    const turn = buildCurrentTurn(
      hostSessionId,
      hostMessageRef,
      text,
      resolveHostWorkspaceRoot(this.api, ctx),
      resolveHostRepoRef(this.api, ctx),
    );
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

  async syncCapabilities(hostSessionId: string, ctx?: Record<string, unknown>): Promise<void> {
    if (!(await this.ensurePeerReady())) {
      return;
    }
    const snapshot = buildCapabilitySnapshot(hostSessionId, this.api, this.gatewayCommandCwd(ctx));
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
        cwd: this.gatewayCommandCwd(),
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
        cwd: this.gatewayCommandCwd(),
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
    options?: { requireBoundTurn?: boolean },
  ): Promise<{ semanticDecisionId?: string; controlActionKind?: string }> {
    if (!(await this.ensurePeerReady())) {
      throw new Error("bridge peer is not active");
    }
    const turn = this.latestTurn(hostSessionId);
    if (!turn && options?.requireBoundTurn !== false) {
      throw new Error(`no current turn bound for session ${hostSessionId}`);
    }
    const canonicalBundle: HostSemanticBundle = {
      ...bundle,
      input_ref: turn?.hostMessageRef ?? bundle.input_ref,
    };
    if (turn?.hostMessageRef && bundle.input_ref !== turn.hostMessageRef) {
      this.api.logger.warn?.("bridge.peer.input_ref_canonicalized", {
        host_session_id: hostSessionId,
        provided_input_ref: bundle.input_ref,
        canonical_input_ref: turn.hostMessageRef,
      });
    }

    const semanticDecision = mapHostSemanticBundleToSemanticDecision(
      canonicalBundle,
      hostSessionId,
      turn?.hostMessageRef,
    );
    const controlAction = mapHostSemanticBundleToControlAction(
      canonicalBundle,
      hostSessionId,
      turn?.hostMessageRef,
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
            cwd: this.gatewayCommandCwd(),
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
        const controlSurface = toControlSurfaceProbeSnapshot(outbound.delivery_id, outbound.payload);
        if (controlSurface) {
          this.controlSurfaceBySession.set(hostSessionId, controlSurface);
        } else if (outbound.payload.type === "result_summary") {
          this.controlSurfaceBySession.delete(hostSessionId);
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
      await runtime.ingestCurrentTurn(
        canonical,
        extractHostMessageRef(event as Record<string, unknown>),
        text,
        ctx as Record<string, unknown>,
      );
    });

    api.on("before_agent_start", async (_event, ctx) => {
      const canonical = resolveCanonicalSession(ctx as Record<string, unknown>);
      if (!canonical || runtime.isExecutionSession(canonical)) {
        return;
      }
      await runtime.syncCapabilities(canonical, ctx as Record<string, unknown>);
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

    api.registerCommand?.({
      name: "loom",
      description: "Operate the Loom slash control surface for this session.",
      acceptsArgs: true,
      requireAuth: true,
      handler: async (ctx) => {
        const probe = resolveCommandSessionProbe(api, ctx);
        const parsedCommand: ParsedLoomCommand | Error = (() => {
          try {
            return parseLoomCommand(ctx);
          } catch (error) {
            return error instanceof Error ? error : new Error(String(error));
          }
        })();
        if (parsedCommand instanceof Error) {
          return { text: `Loom command failed: ${parsedCommand.message}` };
        }
        runtime.recordCommandInvocation(ctx, probe);
        api.logger.info?.("loom.command.invoked", {
          command_body: ctx.commandBody,
          command_verb: parsedCommand.verb,
          resolved_host_session_id: probe.canonical,
          conversation_candidates: probe.conversationCandidates,
          attempts: probe.attempts,
        });
        if (!probe.canonical) {
          return { text: "Loom command failed: unable to resolve host_session_id for this session." };
        }
        runtime.rememberHandledLoomCommand(probe.canonical, ctx.commandBody);
        if (parsedCommand.verb === "probe") {
          return { text: runtime.renderRecordedCommandProbeReport(ctx, probe) };
        }
        try {
          if (parsedCommand.verb === "help") {
            const surface = await runtime.readCurrentControlSurface(probe.canonical);
            return { text: buildControlSurfaceHelpText(surface) };
          }
          const surface = await runtime.readCurrentControlSurface(probe.canonical);
          if (!surface) {
            return { text: buildControlSurfaceHelpText(surface) };
          }
          const bundle = buildControlActionBundle(parsedCommand, surface, ctx.commandBody);
          const result = await runtime.submitBundle(probe.canonical, bundle, {
            requireBoundTurn: false,
          });
          const controlActionKind =
            result.controlActionKind ?? resolveSlashActionKind(parsedCommand, surface);
          return {
            text: [
              `Submitted Loom action: ${controlActionKind}`,
              `managed_task_ref: ${surface.managed_task_ref}`,
              `surface_type: ${surface.surface_type}`,
            ].join("\n"),
          };
        } catch (error) {
          return {
            text: `Loom command failed: ${error instanceof Error ? error.message : String(error)}`,
          };
        }
      },
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
      start: async (ctx) => {
        runtime.setCommandProbeOutputRoot(ctx?.stateDir);
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

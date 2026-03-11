import { createHash, createHmac, randomUUID } from "node:crypto";

import type {
  ApprovalRequestPayload,
  BoundaryCardPayload,
  BridgeBootstrapAck,
  BridgeBootstrapMaterial,
  BridgeBootstrapRequest,
  BridgeHealthResponse,
  BridgeSessionCredential,
  ControlAction,
  CurrentTurnEnvelope,
  HostExecutionCommand,
  HostSubagentLifecycleEnvelope,
  HostSessionId,
  KernelOutboundPayload,
  OutboundDelivery,
  SemanticDecisionEnvelope,
  StartCardPayload,
  ResultSummaryPayload,
  SuppressHostMessagePayload,
  ToolDecisionPayload,
} from "./types.js";
import type { HostCapabilitySnapshot } from "./rustWireTypes.js";

export class BridgeHttpError extends Error {
  readonly method: string;
  readonly path: string;
  readonly status: number;
  readonly body: string;

  constructor(method: string, path: string, status: number, body: string) {
    super(
      body.trim().length > 0
        ? `bridge ${method} ${path} failed with ${status}: ${body.trim()}`
        : `bridge ${method} ${path} failed with ${status}`,
    );
    this.name = "BridgeHttpError";
    this.method = method;
    this.path = path;
    this.status = status;
    this.body = body.trim();
  }
}

export type LoomBridgeClient = {
  health: () => Promise<BridgeHealthResponse>;
  bootstrap: (material: BridgeBootstrapMaterial) => Promise<BridgeBootstrapAck>;
  postCurrentTurn: (payload: CurrentTurnEnvelope) => Promise<void>;
  postCapabilitySnapshot: (payload: HostCapabilitySnapshot) => Promise<void>;
  postSemanticDecision: (payload: SemanticDecisionEnvelope) => Promise<void>;
  postControlAction: (payload: ControlAction) => Promise<void>;
  postSubagentLifecycle: (payload: HostSubagentLifecycleEnvelope) => Promise<void>;
  nextOutbound: (hostSessionId: HostSessionId) => Promise<OutboundDelivery | null>;
  ackOutbound: (deliveryId: string) => Promise<boolean>;
  nextHostExecution: (hostSessionId: HostSessionId) => Promise<HostExecutionCommand | null>;
  ackHostExecution: (commandId: string) => Promise<boolean>;
};

export type LoomBridgeClientOptions = {
  adapterId: string;
  getCredential: () => BridgeSessionCredential | null;
};

function normalizeHostExecutionCommand(raw: Record<string, unknown>): HostExecutionCommand {
  return {
    ...(raw as HostExecutionCommand),
    host_child_execution_ref:
      typeof raw.host_child_execution_ref === "string"
        ? raw.host_child_execution_ref
        : typeof raw.child_session_key === "string"
          ? raw.child_session_key
          : null,
    host_child_run_ref:
      typeof raw.host_child_run_ref === "string"
        ? raw.host_child_run_ref
        : typeof raw.child_run_id === "string"
          ? raw.child_run_id
          : null,
  };
}

function timestamp(): string {
  return Date.now().toString();
}

function bodySha256(body: string): string {
  return createHash("sha256").update(body).digest("hex");
}

function authHeaders(
  method: string,
  path: string,
  body: string,
  credential: BridgeSessionCredential,
): HeadersInit {
  const signedAt = timestamp();
  const nonce = `nonce-${randomUUID()}`;
  const canonical = [method, path, bodySha256(body), signedAt, nonce].join("\n");
  const signature = createHmac("sha256", credential.session_secret)
    .update(canonical)
    .digest("hex");
  return {
    "x-loom-bridge-instance-id": credential.bridge_instance_id,
    "x-loom-adapter-id": credential.adapter_id,
    "x-loom-secret-ref": credential.secret_ref,
    "x-loom-rotation-epoch": String(credential.rotation_epoch),
    "x-loom-signed-at": signedAt,
    "x-loom-nonce": nonce,
    "x-loom-signature": signature,
  };
}

async function responseError(method: string, path: string, response: Response): Promise<BridgeHttpError> {
  return new BridgeHttpError(method, path, response.status, await response.text());
}

async function postJson(
  baseUrl: string,
  path: string,
  payload: unknown,
  options: LoomBridgeClientOptions,
): Promise<void> {
  const credential = options.getCredential();
  if (!credential) {
    throw new Error("bridge client is not bootstrapped");
  }
  const body = JSON.stringify(payload);
  const response = await fetch(new URL(path, baseUrl), {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...authHeaders("POST", path, body, credential),
    },
    body,
  });
  if (!response.ok) {
    throw await responseError("POST", path, response);
  }
}

export function createLoomBridgeClient(
  baseUrl: string,
  options: LoomBridgeClientOptions,
): LoomBridgeClient {
  return {
    async health() {
      const response = await fetch(new URL("/v1/health", baseUrl));
      if (!response.ok) {
        throw await responseError("GET", "/v1/health", response);
      }
      return (await response.json()) as BridgeHealthResponse;
    },
    async bootstrap(material) {
      const payload: BridgeBootstrapRequest = {
        bridge_instance_id: material.bridge_instance_id,
        adapter_id: material.adapter_id,
        ticket_id: material.ticket_id,
        ticket_secret: material.ticket_secret,
        requested_at: timestamp(),
      };
      const response = await fetch(new URL("/v1/bootstrap", baseUrl), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!response.ok) {
        throw await responseError("POST", "/v1/bootstrap", response);
      }
      return (await response.json()) as BridgeBootstrapAck;
    },
    async postCurrentTurn(payload) {
      await postJson(baseUrl, "/v1/ingress/current-turn", payload, options);
    },
    async postCapabilitySnapshot(payload) {
      await postJson(baseUrl, "/v1/ingress/capability-snapshot", payload, options);
    },
    async postSemanticDecision(payload) {
      await postJson(baseUrl, "/v1/ingress/semantic-decision", payload, options);
    },
    async postControlAction(payload) {
      await postJson(baseUrl, "/v1/ingress/control-action", payload, options);
    },
    async postSubagentLifecycle(payload) {
      await postJson(baseUrl, "/v1/ingress/subagent-lifecycle", payload, options);
    },
    async nextOutbound(hostSessionId) {
      const credential = options.getCredential();
      if (!credential) {
        throw new Error("bridge client is not bootstrapped");
      }
      const path = `/v1/outbound/next?host_session_id=${encodeURIComponent(hostSessionId)}`;
      const response = await fetch(new URL(path, baseUrl), {
        headers: authHeaders("GET", path, "", credential),
      });
      if (response.status === 204) {
        return null;
      }
      if (!response.ok) {
        throw await responseError("GET", path, response);
      }
      const raw = (await response.json()) as Omit<OutboundDelivery, "payload"> & {
        payload: Record<string, unknown>;
      };
      return {
        ...raw,
        payload: normalizeKernelOutboundPayload(raw.payload),
      };
    },
    async ackOutbound(deliveryId) {
      const credential = options.getCredential();
      if (!credential) {
        throw new Error("bridge client is not bootstrapped");
      }
      const path = `/v1/outbound/${deliveryId}/ack`;
      const response = await fetch(new URL(path, baseUrl), {
        method: "POST",
        headers: authHeaders("POST", path, "", credential),
      });
      if (response.status === 404) {
        return false;
      }
      if (!response.ok) {
        throw await responseError("POST", path, response);
      }
      return true;
    },
    async nextHostExecution(hostSessionId) {
      const credential = options.getCredential();
      if (!credential) {
        throw new Error("bridge client is not bootstrapped");
      }
      const path = `/v1/host-execution/next?host_session_id=${encodeURIComponent(hostSessionId)}`;
      const response = await fetch(new URL(path, baseUrl), {
        headers: authHeaders("GET", path, "", credential),
      });
      if (response.status === 204) {
        return null;
      }
      if (!response.ok) {
        throw await responseError("GET", path, response);
      }
      return normalizeHostExecutionCommand((await response.json()) as Record<string, unknown>);
    },
    async ackHostExecution(commandId) {
      const credential = options.getCredential();
      if (!credential) {
        throw new Error("bridge client is not bootstrapped");
      }
      const path = `/v1/host-execution/${commandId}/ack`;
      const response = await fetch(new URL(path, baseUrl), {
        method: "POST",
        headers: authHeaders("POST", path, "", credential),
      });
      if (response.status === 404) {
        return false;
      }
      if (!response.ok) {
        throw await responseError("POST", path, response);
      }
      return true;
    },
  };
}

export function normalizeKernelOutboundPayload(
  raw: Record<string, unknown>,
): KernelOutboundPayload {
  const entries = Object.entries(raw);
  if (entries.length !== 1) {
    throw new Error("kernel outbound payload must contain exactly one variant");
  }
  const [variant, data] = entries[0];
  switch (variant) {
    case "StartCard":
    case "start_card":
      return { type: "start_card", data: data as StartCardPayload };
    case "BoundaryCard":
    case "boundary_card":
      return { type: "boundary_card", data: data as BoundaryCardPayload };
    case "ApprovalRequest":
    case "approval_request":
      return { type: "approval_request", data: data as ApprovalRequestPayload };
    case "ResultSummary":
    case "result_summary":
      return { type: "result_summary", data: data as ResultSummaryPayload };
    case "SuppressHostMessage":
    case "suppress_host_message":
      return {
        type: "suppress_host_message",
        data: data as SuppressHostMessagePayload,
      };
    case "ToolDecision":
    case "tool_decision":
      return { type: "tool_decision", data: data as ToolDecisionPayload };
    default:
      throw new Error(`unsupported kernel outbound variant: ${variant}`);
  }
}

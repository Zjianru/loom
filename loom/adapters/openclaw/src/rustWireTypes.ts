export type HostKind = "openclaw";

export type HostAgentCapability = {
  host_agent_ref: string;
  display_name: string;
  available: boolean;
};

export type HostModelCapability = {
  host_model_ref: string;
  provider: string;
  available: boolean;
};

export type HostToolCapability = {
  tool_name: string;
  available: boolean;
};

export type HostSpawnRuntimeKind = "subagent" | "acp";

export type HostSpawnAgentScopeMode = "all" | "explicit_list" | "none" | "unknown";

export type HostSpawnAgentScope = {
  mode: HostSpawnAgentScopeMode;
  allowed_host_agent_refs: string[];
};

export type HostSpawnCapability = {
  runtime_kind: HostSpawnRuntimeKind;
  available: boolean;
  host_agent_scope: HostSpawnAgentScope;
  supports_resume_session: boolean;
  supports_thread_spawn: boolean;
  supports_parent_progress_stream: boolean;
};

export type HostSessionRole = "main" | "orchestrator" | "leaf" | "unknown";

export type HostSessionControlScope = "children" | "none" | "unknown";

export type HostCapabilityFactSource = "authoritative" | "derived" | "unknown";

export type HostSessionCapabilityScope = {
  session_role: HostSessionRole;
  control_scope: HostSessionControlScope;
  source: HostCapabilityFactSource;
};

export type HostRenderCapabilities = {
  supports_text_render: boolean;
  supports_inline_actions: boolean;
  supports_message_suppression: boolean;
};

export type HostWorkerControlCapabilities = {
  supports_pause: boolean;
  supports_resume: boolean;
  supports_cancel: boolean;
  supports_soft_interrupt: boolean;
  supports_hard_interrupt: boolean;
};

export type HostCapabilitySnapshot = {
  capability_snapshot_ref: string;
  host_kind: HostKind;
  host_session_id: string;
  available_agents: HostAgentCapability[];
  available_models: HostModelCapability[];
  available_tools: HostToolCapability[];
  spawn_capabilities: HostSpawnCapability[];
  session_scope: HostSessionCapabilityScope;
  allowed_tools: string[];
  readable_roots: string[];
  writable_roots: string[];
  secret_classes: string[];
  max_budget_band: "conservative" | "standard" | "elevated";
  render_capabilities: HostRenderCapabilities;
  background_task_support: boolean;
  async_notice_support: boolean;
  available_agent_ids: string[];
  supports_spawn_agents: boolean;
  supports_pause: boolean;
  supports_resume: boolean;
  supports_interrupt: boolean;
  worker_control_capabilities: HostWorkerControlCapabilities;
  recorded_at: string;
};

export type HostCapabilitySnapshot = {
  capability_snapshot_ref: string;
  host_session_id: string;
  allowed_tools: string[];
  readable_roots: string[];
  writable_roots: string[];
  secret_classes: string[];
  max_budget_band: "conservative" | "standard" | "elevated";
  available_agent_ids: string[];
  supports_spawn_agents: boolean;
  supports_pause: boolean;
  supports_resume: boolean;
  supports_interrupt: boolean;
  recorded_at: string;
};

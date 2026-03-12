import type { KernelOutboundPayload } from "./types.js";

function renderSlashCommands(actions: string[]): string {
  const commands = actions.map((action) => {
    switch (action) {
      case "approve_start":
      case "approve_request":
        return "/loom approve";
      case "modify_candidate":
        return "/loom modify <summary>";
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
  });
  return commands.join(" | ");
}

export function renderPayload(payload: KernelOutboundPayload): string {
  switch (payload.type) {
    case "start_card":
      return [
        `Task: ${payload.data.title}`,
        `Ref: ${payload.data.managed_task_ref}`,
        `Class: ${payload.data.managed_task_class}`,
        `Horizon: ${payload.data.work_horizon}`,
        `Summary: ${payload.data.summary}`,
        `Outcome: ${payload.data.expected_outcome}`,
        `Commands: ${renderSlashCommands(payload.data.allowed_actions)}`,
      ].join("\n");
    case "boundary_card":
      return [
        `Current task: ${payload.data.managed_task_ref}`,
        `Candidate task: ${payload.data.candidate_managed_task_ref}`,
        `Active summary: ${payload.data.active_task_summary}`,
        `Candidate summary: ${payload.data.candidate_task_summary}`,
        `Reason: ${payload.data.boundary_reason}`,
        `Commands: ${renderSlashCommands(payload.data.allowed_actions)}`,
      ].join("\n");
    case "approval_request":
      return [
        `Approval requested for ${payload.data.managed_task_ref}`,
        `Scope: ${payload.data.approval_scope}`,
        `Why now: ${payload.data.why_now}`,
        `Risk: ${payload.data.risk_summary}`,
        `Commands: ${renderSlashCommands(payload.data.allowed_actions)}`,
      ].join("\n");
    case "result_summary":
      return [
        `Task: ${payload.data.managed_task_ref}`,
        `Outcome: ${payload.data.outcome}`,
        `Acceptance: ${payload.data.acceptance_verdict}`,
        `Scope version: ${payload.data.final_scope_version}`,
        `Summary: ${payload.data.summary}`,
        `Scope headline: ${payload.data.scope_revision_headline ?? "n/a"}`,
        `Proof: ${payload.data.proof_of_work_excerpt.run_summary}`,
        `Evidence: ${payload.data.proof_of_work_excerpt.evidence_refs.map((item) => `${item.label}=${item.reference}`).join(", ") || "n/a"}`,
        `Next actions: ${payload.data.next_actions_excerpt.map((item) => item.title).join(", ") || "n/a"}`,
      ].join("\n");
    case "suppress_host_message":
      return `Suppress host message: ${payload.data.reason}`;
    case "tool_decision":
      return [
        `Task: ${payload.data.managed_task_ref}`,
        `Decision: ${payload.data.decision_value}`,
        `Area: ${payload.data.decision_area}`,
        `Summary: ${payload.data.summary}`,
      ].join("\n");
    case "status_notice":
      return [
        `Notice: ${payload.data.headline}`,
        `Task: ${payload.data.managed_task_ref}`,
        `Kind: ${payload.data.notice_kind}`,
        `Stage ref: ${payload.data.stage_ref}`,
        `Summary: ${payload.data.summary}`,
        `Detail: ${payload.data.detail ?? "n/a"}`,
      ].join("\n");
  }
}

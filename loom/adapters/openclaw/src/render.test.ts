import { describe, expect, it } from "vitest";

import { renderPayload } from "./render.js";
import type { KernelOutboundPayload } from "./types.js";

describe("renderPayload", () => {
  it("renders start card from formal payload fields only", () => {
    const payload: KernelOutboundPayload = {
      type: "start_card",
      data: {
        managed_task_ref: "task-1",
        decision_token: "decision-1",
        managed_task_class: "COMPLEX",
        work_horizon: "improvement",
        task_activation_reason: "explicit_start_task",
        title: "Refactor risk chain",
        summary: "Implement the first-layer risk chain.",
        expected_outcome: "Scope, risk and auth become durable.",
        recommended_pack_ref: "coding_pack",
        allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
      },
    };

    const rendered = renderPayload(payload);
    expect(rendered).toContain("Ref: task-1");
    expect(rendered).toContain("Class: COMPLEX");
    expect(rendered).toContain("Horizon: improvement");
    expect(rendered).toContain("Summary: Implement the first-layer risk chain.");
    expect(rendered).toContain("/loom approve");
    expect(rendered).toContain("/loom modify");
    expect(rendered).toContain("/loom cancel");
    expect(rendered).not.toContain("Token:");
  });

  it("renders approval request without inventing extra governance fields", () => {
    const payload: KernelOutboundPayload = {
      type: "approval_request",
      data: {
        managed_task_ref: "task-2",
        decision_token: "decision-2",
        approval_scope: "tool_execution",
        allowed_actions: ["approve_request", "reject_request"],
        why_now: "critical irreversible action",
        risk_summary: "overall_risk_band=critical",
      },
    };

    const rendered = renderPayload(payload);
    expect(rendered).toContain("Approval requested for task-2");
    expect(rendered).toContain("Why now: critical irreversible action");
    expect(rendered).toContain("Risk: overall_risk_band=critical");
    expect(rendered).toContain("/loom approve");
    expect(rendered).toContain("/loom reject");
    expect(rendered).not.toContain("Token:");
  });

  it("renders boundary confirmation with executable /loom commands instead of token leakage", () => {
    const payload: KernelOutboundPayload = {
      type: "boundary_card",
      data: {
        managed_task_ref: "task-3",
        candidate_managed_task_ref: "task-4",
        decision_token: "decision-3",
        active_task_summary: "Keep working on the active task.",
        candidate_task_summary: "Switch to the new task candidate.",
        boundary_reason: "existing_task_active",
        allowed_actions: ["keep_current_task", "replace_active"],
      },
    };

    const rendered = renderPayload(payload);
    expect(rendered).toContain("Current task: task-3");
    expect(rendered).toContain("Candidate task: task-4");
    expect(rendered).toContain("/loom keep");
    expect(rendered).toContain("/loom replace");
    expect(rendered).not.toContain("Token:");
  });

  it("renders result summary with proof and next actions excerpts", () => {
    const payload: KernelOutboundPayload = {
      type: "result_summary",
      data: {
        managed_task_ref: "task-3",
        outcome: "completed",
        acceptance_verdict: "accepted",
        summary: "First-round closed loop completed.",
        final_scope_version: 1,
        scope_revision_headline: "scope_version=1",
        proof_of_work_excerpt: {
          run_summary: "worker + review + recorder chain completed",
          evidence_refs: [{ label: "run_ref", reference: "run-1" }],
          review_summary: {
            review_verdict: "approved",
            summary: "Worker output passed review.",
            key_findings: [],
            follow_up_required: false,
          },
          artifact_manifest_excerpt: [{ label: "artifact", reference: "/Users/codez/.openclaw/loom/README.md" }],
          acceptance_basis_excerpt: ["Review approved the worker execution."],
        },
        next_actions_excerpt: [{ title: "Verify delivered summary", details: "Confirm the WebUI summary." }],
      },
    };

    const rendered = renderPayload(payload);
    expect(rendered).toContain("Outcome: completed");
    expect(rendered).toContain("Proof: worker + review + recorder chain completed");
    expect(rendered).toContain("Evidence: run_ref=run-1");
    expect(rendered).toContain("Next actions: Verify delivered summary");
  });

  it("renders status notice with headline, stage ref, and detail", () => {
    const payload: KernelOutboundPayload = {
      type: "status_notice",
      data: {
        managed_task_ref: "task-4",
        notice_kind: "blocked",
        stage_ref: "phase-entry-execute",
        headline: "Execute stage blocked",
        summary: "Task could not enter execute because the host bridge is missing required agents.",
        detail: "Missing worker or recorder host mapping.",
      },
    };

    const rendered = renderPayload(payload);
    expect(rendered).toContain("Notice: Execute stage blocked");
    expect(rendered).toContain("Task: task-4");
    expect(rendered).toContain("Kind: blocked");
    expect(rendered).toContain("Stage ref: phase-entry-execute");
    expect(rendered).toContain("Detail: Missing worker or recorder host mapping.");
  });
});

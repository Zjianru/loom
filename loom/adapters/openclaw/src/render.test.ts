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
        task_activation_reason: "explicit_user_request",
        title: "Refactor risk chain",
        summary: "Implement the first-layer risk chain.",
        expected_outcome: "Scope, risk and auth become durable.",
        recommended_pack_ref: "coding_pack",
        allowed_actions: ["approve_start", "modify_candidate", "cancel_candidate"],
      },
    };

    const rendered = renderPayload(payload);
    expect(rendered).toContain("Ref: task-1");
    expect(rendered).toContain("Token: decision-1");
    expect(rendered).toContain("Class: COMPLEX");
    expect(rendered).toContain("Horizon: improvement");
    expect(rendered).toContain("Summary: Implement the first-layer risk chain.");
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
    expect(rendered).toContain("Token: decision-2");
    expect(rendered).toContain("Why now: critical irreversible action");
    expect(rendered).toContain("Risk: overall_risk_band=critical");
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
});

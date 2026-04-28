/* Client-side mirror of `yah/src/arch/ticket.rs::to_prompt_with_ctx`.
   The card-level Prompt button generates this so a pickup or review
   agent can be launched from the clipboard without round-tripping the
   server. The Rust function is the source of truth — the stretch goal
   is a server-side prompt RPC so this stays in lockstep. For now the
   shapes diverge: this v1 only sees the ticket itself, so it skips
   inherited-from-parent gotchas/verify and the sub-tickets section. */

import type { Ticket } from "../../types";

const SHELL_PREFIXES = [
  "yah ",
  "cargo ",
  "bun ",
  "bunx ",
  "npm ",
  "npx ",
  "git ",
  "rg ",
  "ls ",
  "make ",
  "./",
];

function looksLikeShellCommand(s: string): boolean {
  const t = s.trim();
  if (!t) return false;
  return SHELL_PREFIXES.some((p) => t.startsWith(p));
}

function stripTrailingComment(s: string): string {
  // Mirrors the Rust strip_trailing_comment well enough for combined-smoke
  // assembly: drop a trailing "  # …" or "  // …" tail.
  const idx = s.search(/\s+(#|\/\/).*$/);
  return idx >= 0 ? s.slice(0, idx).trimEnd() : s.trimEnd();
}

export function buildPickupPrompt(t: Ticket): string {
  const out: string[] = [];

  out.push(`# Continue: ${t.id} — ${t.title}\n\n`);

  if (t.gotchas?.length) {
    out.push("## ⚠ Gotchas (read first)\n\n");
    for (const g of t.gotchas) out.push(`- ${g}\n`);
    out.push("\n");
  }

  out.push("## Context\n\n");
  out.push(`\`${t.id}\` `);
  if (t.assignee) out.push(`(from ${t.assignee}) `);
  out.push("is ready for continuation.\n\n");

  if (t.phase) out.push(`**Phase**: ${t.phase}\n\n`);
  if (t.parent) out.push(`**Parent relay**: ${t.parent}\n\n`);

  if (t.handoff?.length) {
    out.push("## What was completed\n\n");
    if (t.handoff.length === 1) {
      out.push(t.handoff[0]);
      out.push("\n\n");
    } else {
      for (const h of t.handoff) out.push(`- ${h}\n`);
      out.push("\n");
    }
  }

  if (t.nextSteps?.length) {
    out.push("## Next steps\n\n");
    for (const step of t.nextSteps) out.push(`- ${step}\n`);
    out.push("\n");
  }

  if (t.verify?.length) {
    out.push("## Verification\n\n");
    const cmdChain: string[] = [];
    let lastWasProse = false;
    for (const v of t.verify) {
      if (looksLikeShellCommand(v)) {
        out.push("```bash\n");
        out.push(v);
        out.push("\n```\n\n");
        cmdChain.push(stripTrailingComment(v));
        lastWasProse = false;
      } else {
        out.push(`- ${v}\n`);
        lastWasProse = true;
      }
    }
    if (lastWasProse) out.push("\n");
    if (cmdChain.length > 1) {
      out.push("Combined smoke test:\n\n```bash\n");
      out.push(cmdChain.join(" && "));
      out.push("\n```\n\n");
    }
  }

  out.push("## Source\n\n");
  out.push(`Defined at \`${t.file}:${t.line}\`\n\n`);

  out.push("## First action\n\n");
  switch (t.status) {
    case "open":
      out.push(
        `Claim this ticket — one atomic command flips status and assignee (Rule01):\n\n` +
          "```bash\n" +
          `yah board claim ${t.id}\n` +
          "```\n\n" +
          "The Prompt button's clipboard copy does **not** move the card for you. " +
          "Run the claim before any other code edits.\n\n",
      );
      break;
    case "handoff":
      out.push(
        `Pick up the baton — one atomic command flips status and assignee (Rule01):\n\n` +
          "```bash\n" +
          `yah board move ${t.id} active\n` +
          "```\n\n" +
          "The Prompt button's clipboard copy does **not** move the card for you. " +
          "Run the move before any other code edits.\n\n",
      );
      break;
    case "claimed":
    case "in-progress":
      out.push(
        `This ticket is already \`${t.status}\` — you're continuing an in-flight ` +
          `session, no claim needed. Begin with the next steps below.\n\n`,
      );
      break;
    case "review":
    case "done":
      out.push(
        `This ticket is already in \`${t.status}\`. If it needs more work, send it ` +
          `back with \`yah board move ${t.id} handoff --handoff "what still needs ` +
          `doing"\`. Otherwise use the review-mode prompt from the card's Review ` +
          `button.\n\n`,
      );
      break;
  }

  out.push("## Playbook\n\n");
  out.push(
    "Load-bearing rules for this pickup: **Rule01** (claim first — above), " +
      "**Col01** (three end-states — below). Full ruleset: " +
      "`yah board rules --context pickup` (or `finishing` when you wrap up).\n\n",
  );
  out.push(
    "Inspect any related ticket: `yah board show <ID>` (compact view) or " +
      "`yah board show <ID> --prompt` (full pickup form, like this one).\n\n",
  );

  out.push("## Then\n\n");
  out.push("1. Read the reference docs and source context above.\n");
  out.push("2. Complete the next steps listed.\n");
  out.push("3. Pick the right end-state (Col01):\n");
  out.push(
    `   - **More work remains (another phase, another agent):** ` +
      `\`yah board move ${t.id} handoff --handoff "what you just finished" --next "first concrete next step"\` ` +
      `— same R-number, baton moves forward in place (Rule03).\n`,
  );
  out.push(
    `   - **This ticket's tasks are met, awaiting human sign-off:** ` +
      `\`yah board move ${t.id} review\` and ping the user. Do **not** self-archive — ` +
      `review is where a human exercises \`@yah:verify(...)\` and confirms.\n`,
  );
  out.push(
    "   - **Already signed off in a previous pass:** archive via the card button " +
      "(strips `@yah:` lines from source, appends `archived` to `.yah/events.jsonl`).\n",
  );

  return out.join("");
}

/* Review-mode prompt: framed for a verifier, not a continuation. The verify
   commands are the canonical task; the agent's job is to run them, then
   either approve (archive via card button) or send back to handoff. */
export function buildReviewPrompt(t: Ticket): string {
  const out: string[] = [];

  out.push(`# Review: ${t.id} — ${t.title}\n\n`);

  out.push("## Context\n\n");
  out.push(`\`${t.id}\` `);
  if (t.assignee) out.push(`(from ${t.assignee}) `);
  out.push("is awaiting review.\n\n");
  if (t.parent) out.push(`**Parent relay**: ${t.parent}\n\n`);

  if (t.handoff?.length) {
    out.push("## What was claimed\n\n");
    if (t.handoff.length === 1) {
      out.push(t.handoff[0]);
      out.push("\n\n");
    } else {
      for (const h of t.handoff) out.push(`- ${h}\n`);
      out.push("\n");
    }
  }

  if (t.verify?.length) {
    out.push("## Verify\n\n");
    out.push("Run each command. If any fail, the ticket is not ready.\n\n");
    const cmdChain: string[] = [];
    let lastWasProse = false;
    for (const v of t.verify) {
      if (looksLikeShellCommand(v)) {
        out.push("```bash\n");
        out.push(v);
        out.push("\n```\n\n");
        cmdChain.push(stripTrailingComment(v));
        lastWasProse = false;
      } else {
        out.push(`- ${v}\n`);
        lastWasProse = true;
      }
    }
    if (lastWasProse) out.push("\n");
    if (cmdChain.length > 1) {
      out.push("Combined smoke test:\n\n```bash\n");
      out.push(cmdChain.join(" && "));
      out.push("\n```\n\n");
    }
  } else {
    out.push("## Verify\n\n");
    out.push(
      "No `@yah:verify(...)` commands declared. Read the diff for this ticket " +
        "and exercise the change manually before deciding.\n\n",
    );
  }

  out.push("## Source\n\n");
  out.push(`Defined at \`${t.file}:${t.line}\`\n\n`);

  out.push("## Decide\n\n");
  out.push(
    "- **Approve:** click the card's archive button (two-stage: arms first, " +
      "then commits — strips `@yah:` lines from source, appends `archived` to " +
      "`.yah/events.jsonl`).\n",
  );
  out.push(
    `- **Send back:** \`yah board move ${t.id} handoff --handoff "what still ` +
      `needs doing"\` — the relay returns to the next pickup with your notes ` +
      "attached.\n",
  );

  return out.join("");
}

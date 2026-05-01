// Realistic mock data drawn from .yah/arch/authored/design-session-handoff/03-sample-data.md.
// When the backend is wired, replace these with API calls; component shapes
// should not change.

import type { ArchSubgraph, Rig, Session, Ticket } from "./types";

// Three local rigs day-one (R024) so multi-rig flow is exercisable in
// dev before the Tauri attach commands round-trip real folders. The
// remotes stay on the list so the selector can still render the
// host/reachable axes. needsAttention seeds let R024-T3's brass pill +
// title-bar pip render without waiting on backend wiring.
export const mockRigs: Rig[] = [
  {
    id: "local",
    name: "synth-engine",
    kind: "local",
    path: "/Users/leif/ss/synth-engine",
    reachable: true,
    needsAttention: 2,
    lastActiveAt: Date.now() - 30_000,
  },
  {
    id: "rs-hack",
    name: "rs-hack",
    kind: "local",
    path: "/Users/leif/ss/rs-hack",
    reachable: true,
    needsAttention: 0,
    lastActiveAt: Date.now() - 4 * 3600_000,
  },
  {
    id: "yah-design",
    name: "yah-design",
    kind: "local",
    path: "/Users/leif/ss/rs-hack/yah-design",
    reachable: true,
    needsAttention: 1,
    lastActiveAt: Date.now() - 26 * 3600_000,
  },
  {
    id: "droplet-1",
    name: "vps-frankfurt",
    kind: "remote",
    host: "leif@10.4.2.21",
    reachable: true,
  },
  {
    id: "homelab",
    name: "homelab-box",
    kind: "remote",
    host: "leif@homelab.local",
    reachable: false,
  },
];

// Recency seeds let the Board sort (R025-T2: last-touched desc primary)
// be visible in dev — without these every mock has lastModifiedTs=0
// and the column would collapse to id-asc only. Values are unix seconds.
const NOW_S = Math.floor(Date.now() / 1000);
const MIN = 60;
const HR = 3600;
const DAY = 24 * HR;

export const mockTickets: Ticket[] = [
  {
    id: "R012",
    title: "Container-aware pickup prompts",
    itemType: "relay",
    kind: "epic",
    status: "in-progress",
    assignee: "agent:claude",
    isZone: true,
    childCounts: { open: 1, active: 1, handoff: 1 },
    file: "src/ticket.rs",
    line: 21,
    lastModifiedTs: NOW_S - 5 * MIN,
    handoff: [
      "Container tickets should be 'watering hole' prompts. Fresh agent arriving at a zone sees the child list, is pointed at the next live child, can come back to claim more.",
    ],
    nextSteps: [
      "Extend to_prompt_with_context so zones walk one level deeper",
      "BUG: next-live picker filters Open|Handoff only, skips InProgress children",
    ],
    gotchas: ["Sub-ticket prompts don't inherit parent gotchas yet"],
    verify: ["cargo test -p yah ticket::"],
  },
  {
    id: "R012-T1",
    title: "Inherit parent gotchas in sub-ticket prompts",
    itemType: "ticket",
    kind: "task",
    status: "review",
    parent: "R012",
    assignee: "agent:claude",
    file: "src/ticket.rs",
    line: 663,
    handoff: ["Added gotcha-inheritance pass in to_prompt_with_ctx."],
    nextSteps: ["Apply same pattern to verify commands"],
    lastModifiedTs: NOW_S - 2 * HR,
  },
  {
    id: "R012-T2",
    title: "Inherit parent verify commands",
    itemType: "ticket",
    kind: "task",
    status: "in-progress",
    parent: "R012",
    assignee: "agent:claude",
    file: "src/ticket.rs",
    line: 720,
    lastModifiedTs: NOW_S - 3 * MIN,
  },
  {
    id: "R012-T3",
    title: "Two-level walk for zone prompts",
    itemType: "ticket",
    kind: "task",
    status: "open",
    parent: "R012",
    file: "src/ticket.rs",
    line: 800,
    lastModifiedTs: NOW_S - 4 * HR,
  },
  {
    id: "R007",
    title: "Multi-worktree event-log sync",
    itemType: "relay",
    status: "handoff",
    assignee: "agent:claude",
    file: "src/status.rs",
    line: 12,
    handoff: [
      "Per-relay event shards land in .yah/events/. Cross-worktree merging via timestamp-sort works for the simple case but races on concurrent appends.",
    ],
    nextSteps: ["File-lock during append", "Conflict surface in board UI"],
    lastModifiedTs: NOW_S - 30 * MIN,
  },
  {
    id: "R009",
    title: "TS annotation scanning (cross-language rigs)",
    itemType: "relay",
    status: "handoff",
    file: "src/extract.rs",
    line: 8,
    handoff: ["Parser stub emits the same ArchAnnotation struct for TS sources via tree-sitter."],
    lastModifiedTs: NOW_S - 18 * HR,
  },
  {
    id: "R013",
    title: "JIT mermaid endpoint for architecture tab",
    itemType: "relay",
    status: "open",
    file: "src/graph.rs",
    line: 425,
    lastModifiedTs: NOW_S - 3 * DAY,
  },
  {
    id: "F003",
    title: "Card archive button two-step confirm",
    itemType: "ticket",
    kind: "feature",
    status: "open",
    file: "src/components/board/TicketCard.tsx",
    line: 1,
    lastModifiedTs: NOW_S - 6 * HR,
  },
  {
    id: "B007",
    title: "prompt button copies stale text after column move",
    itemType: "ticket",
    kind: "bug",
    status: "open",
    file: "src/server.ts",
    line: 1240,
    lastModifiedTs: NOW_S - 12 * HR,
  },
  {
    id: "R005",
    title: "yah-board: two-noun model",
    itemType: "relay",
    status: "review",
    assignee: "agent:claude",
    file: "src/ticket.rs",
    line: 5,
    handoff: ["Refactored to two-noun model: Ticket + Relay."],
    lastModifiedTs: NOW_S - 5 * DAY,
  },
];

export const mockArchSubgraph: ArchSubgraph = {
  rootId: "voice_allocator",
  depth: 2,
  nodes: [
    { id: "voice_allocator", shortName: "voice_allocator", layer: "audio", roles: ["allocator"], file: "src/voice_allocator.rs", line: 1, doc: "Voice allocator — owns polyphonic state for the synth engine." },
    { id: "impulse_hub", shortName: "ImpulseHub", layer: "dispatch", roles: ["gateway"], file: "src/impulse.rs", line: 14 },
    { id: "envelope", shortName: "Envelope", layer: "audio", roles: ["dsp"], file: "src/envelope.rs", line: 1 },
    { id: "filter", shortName: "Filter", layer: "audio", roles: ["dsp"], file: "src/filter.rs", line: 1 },
    { id: "midi_input", shortName: "MidiInput", layer: "io", roles: ["gateway"], file: "src/midi.rs", line: 8 },
    { id: "system_state", shortName: "SystemState", layer: "state", roles: ["aggregate_root"], file: "src/state.rs", line: 1 },
    { id: "preset_store", shortName: "PresetStore", layer: "state", roles: ["repo"], file: "src/presets.rs", line: 1 },
  ],
  edges: [
    { from: "midi_input", to: "impulse_hub", kind: "data_flow" },
    { from: "impulse_hub", to: "voice_allocator", kind: "message_flow", reason: "impulse:NoteOn" },
    { from: "voice_allocator", to: "envelope", kind: "depends_on" },
    { from: "voice_allocator", to: "filter", kind: "depends_on" },
    { from: "voice_allocator", to: "system_state", kind: "context", reason: "provides voice-state" },
    { from: "preset_store", to: "voice_allocator", kind: "context", reason: "provides preset" },
  ],
};

export const mockSession: Session = {
  relayId: "R012-T2",
  model: "claude-sonnet-4-7",
  tokens: 8420,
  status: "streaming",
  lastActive: Date.now() - 18_000,
  events: [
    {
      id: "e1",
      t: Date.now() - 60_000,
      role: "user",
      content: "Pick up R012-T2. Apply the gotcha-inheritance pattern to verify commands too.",
    },
    {
      id: "e2",
      t: Date.now() - 58_000,
      role: "assistant",
      type: "thinking",
      content: "Looking at how gotcha inheritance was implemented in R012-T1 — the pattern uses a parent-walk in to_prompt_with_ctx and concatenates inherited blocks above the local context block. Should replicate for verify commands.",
    },
    {
      id: "e3",
      t: Date.now() - 55_000,
      role: "assistant",
      type: "tool_use",
      tool: "read",
      args: { path: "src/ticket.rs", range: [600, 720] },
    },
    {
      id: "e4",
      t: Date.now() - 54_000,
      role: "tool",
      tool: "read",
      result: { lines: 120, summary: "to_prompt_with_ctx + helpers" },
    },
    {
      id: "e5",
      t: Date.now() - 50_000,
      role: "assistant",
      type: "tool_use",
      tool: "grep",
      args: { pattern: "inherited from", glob: "*.rs" },
    },
    {
      id: "e6",
      t: Date.now() - 49_000,
      role: "tool",
      tool: "grep",
      result: [
        { file: "src/ticket.rs", line: 663, text: "## ⚠ Gotchas inherited from" },
      ],
    },
    {
      id: "e7",
      t: Date.now() - 40_000,
      role: "assistant",
      type: "tool_use",
      tool: "edit",
      args: {
        path: "src/ticket.rs",
        diff:
          "+ if let Some(parent_verify) = parent.verify_smoke() {\n+     out.push_str(\"## Verification inherited from \");\n+     out.push_str(parent_id);\n+     out.push_str(\"\\n\\n```bash\\n\");\n+     out.push_str(&parent_verify);\n+     out.push_str(\"\\n```\\n\\n\");\n+ }\n",
      },
    },
    {
      id: "e8",
      t: Date.now() - 39_000,
      role: "tool",
      tool: "edit",
      result: { ok: true, additions: 8, deletions: 0 },
    },
    {
      id: "e9",
      t: Date.now() - 30_000,
      role: "assistant",
      type: "text",
      content:
        "Added verify-inheritance helper at `src/ticket.rs:663` mirroring the gotcha pass. Running tests.",
    },
    {
      id: "e10",
      t: Date.now() - 28_000,
      role: "assistant",
      type: "tool_use",
      tool: "bash",
      args: { cmd: "cargo test -p yah ticket::" },
    },
    {
      id: "e11",
      t: Date.now() - 18_000,
      role: "tool",
      tool: "bash",
      result: {
        stdout:
          "running 26 tests\ntest ticket::test_picker_picks_in_progress_child ... ok\ntest ticket::test_subticket_inherits_parent_verify ... ok\n... (24 more)\ntest result: ok. 26 passed; 0 failed",
        exit: 0,
      },
    },
    {
      id: "e12",
      t: Date.now() - 17_000,
      role: "assistant",
      type: "text",
      content:
        "All 26 tests pass — including the new `test_subticket_inherits_parent_verify`. Ready to handoff.",
    },
  ],
};

export const mockSessionList: { relayId: string; title: string; status: Session["status"]; lastActive: number; model: string }[] =
  [
    { relayId: "R012-T2", title: "Inherit parent verify commands", status: "streaming", lastActive: Date.now() - 18_000, model: "claude-sonnet-4-7" },
    { relayId: "R007", title: "Multi-worktree event-log sync", status: "idle", lastActive: Date.now() - 6 * 3600_000, model: "claude-opus-4-7" },
    { relayId: "R009", title: "TS annotation scanning", status: "idle", lastActive: Date.now() - 26 * 3600_000, model: "claude-sonnet-4-7" },
  ];

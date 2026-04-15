/**
 * hack-board server
 *
 * Serves the kanban UI and provides a real-time ticket feed:
 * - GET /              → static UI
 * - GET /api/tickets   → current ticket JSON
 * - GET /api/events    → SSE stream of ticket updates
 * - POST /api/nudge    → manual re-scan trigger
 *
 * File watcher (Bun fs.watch) + UDP listener both trigger re-scans.
 * Source files are the only source of truth.
 */

import { watch } from "fs";
import { createSocket } from "dgram";
import { join, resolve } from "path";
import { $ } from "bun";

// ── Config ──────────────────────────────────────────────────────────────

const PORT = parseInt(process.env.HACK_PORT || "3333");
const UDP_PORT = parseInt(process.env.HACK_UDP_PORT || "3334");
const WORKSPACE = resolve(process.env.HACK_WORKSPACE || process.cwd());
// Prefer local dev build, fall back to installed rs-hack
const RS_HACK = process.env.RS_HACK_BIN || (() => {
  const devBin = join(WORKSPACE, "target", "debug", "rs-hack");
  try {
    const stat = Bun.file(devBin);
    // We can't synchronously check existence in all Bun versions,
    // so just try the dev path first at runtime
    return devBin;
  } catch {
    return "rs-hack";
  }
})();
const DEBOUNCE_MS = 300;

// ── Ticket Scanner ──────────────────────────────────────────────────────

interface Ticket {
  id: string;
  title: string;
  item_type: "ticket" | "relay";
  kind?: string;
  status: string;
  assignee?: string;
  phase?: string;
  parent?: string;
  severity?: string;
  handoff?: string;
  next_steps?: string[];
  cleanup?: string[];
  verify?: string[];
  depends_on: string[];
  see_also: string[];
  file: string;
  line: number;
}

interface Summary {
  id: string;
  ticket?: string;
  author?: string;
  timestamp: number;
  text: string;
  file: string;
  promoted: boolean;
  relay_id?: string;
  relay_title?: string;
}

let currentTickets: Ticket[] = [];
let currentSummaries: Summary[] = [];
let scanCount = 0;

async function scanTickets(): Promise<Ticket[]> {
  try {
    const result = await $`${RS_HACK} arch tickets -f json -p ${WORKSPACE}`
      .text();
    const tickets = JSON.parse(result) as Ticket[];
    scanCount++;
    return tickets;
  } catch (e) {
    console.error(`[scan] failed:`, e);
    return currentTickets; // keep last good state
  }
}

async function scanSummaries(): Promise<Summary[]> {
  const dir = join(WORKSPACE, ".hack", "summaries");
  try {
    const glob = new Bun.Glob("*.md");
    const summaries: Summary[] = [];
    for await (const path of glob.scan(dir)) {
      try {
        const content = await Bun.file(join(dir, path)).text();
        const summary = parseSummary(path, content);
        if (summary) summaries.push(summary);
      } catch {}
    }
    summaries.sort((a, b) => b.timestamp - a.timestamp);
    return summaries;
  } catch {
    return [];
  }
}

function parseSummary(
  filename: string,
  content: string
): Summary | null {
  const id = filename.replace(/\.md$/, "");
  if (!content.startsWith("---\n")) {
    return {
      id,
      timestamp: 0,
      text: content.trim(),
      file: filename,
      promoted: false,
    };
  }
  const endIdx = content.indexOf("---\n", 4);
  if (endIdx === -1) return null;

  const frontmatter = content.slice(4, endIdx);
  const body = content.slice(endIdx + 4).trim();

  const fm: Record<string, string> = {};
  for (const line of frontmatter.split("\n")) {
    const colonIdx = line.indexOf(":");
    if (colonIdx > 0) {
      fm[line.slice(0, colonIdx).trim()] = line.slice(colonIdx + 1).trim();
    }
  }

  return {
    id,
    ticket: fm.ticket || undefined,
    author: fm.author || undefined,
    timestamp: parseInt(fm.timestamp || "0"),
    text: body,
    file: filename,
    promoted: fm.promoted === "true",
    relay_id: fm.relay_id || undefined,
    relay_title: fm.relay_title || undefined,
  };
}

// ── SSE Clients ─────────────────────────────────────────────────────────

const sseClients = new Set<ReadableStreamDefaultController>();

function broadcast(tickets: Ticket[], summaries: Summary[]) {
  const data = JSON.stringify({ tickets, summaries });
  const msg = `data: ${data}\n\n`;
  for (const controller of sseClients) {
    try {
      controller.enqueue(new TextEncoder().encode(msg));
    } catch {
      sseClients.delete(controller);
    }
  }
}

// ── Debounced Re-scan ───────────────────────────────────────────────────

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

function triggerRescan(reason: string) {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(async () => {
    console.log(`[scan] triggered by ${reason}`);
    const [tickets, summaries] = await Promise.all([
      scanTickets(),
      scanSummaries(),
    ]);
    currentTickets = tickets;
    currentSummaries = summaries;
    broadcast(tickets, summaries);
  }, DEBOUNCE_MS);
}

// ── File Watcher ────────────────────────────────────────────────────────

console.log(`[watch] ${WORKSPACE}`);
const watcher = watch(WORKSPACE, { recursive: true }, (event, filename) => {
  if (
    filename &&
    (filename.endsWith(".rs") || filename.includes(".hack/summaries/"))
  ) {
    triggerRescan(`fs:${filename}`);
  }
});

// ── UDP Listener (fire-and-forget nudge from rs-hack) ───────────────────

const udp = createSocket("udp4");
udp.on("message", (msg) => {
  triggerRescan(`udp:${msg.toString().trim()}`);
});
udp.on("error", (err) => {
  console.error(`[udp] error:`, err);
});
udp.bind(UDP_PORT, "127.0.0.1", () => {
  console.log(`[udp] listening on 127.0.0.1:${UDP_PORT}`);
});

// ── HTTP Server ─────────────────────────────────────────────────────────

const publicDir = join(import.meta.dir, "..", "public");

const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);

    // API: get current tickets
    if (url.pathname === "/api/tickets") {
      return Response.json(currentTickets);
    }

    // API: get summaries
    if (url.pathname === "/api/summaries") {
      return Response.json(currentSummaries);
    }

    // API: SSE event stream
    if (url.pathname === "/api/events") {
      const stream = new ReadableStream({
        start(controller) {
          sseClients.add(controller);
          // Send current state immediately
          const data = JSON.stringify({
            tickets: currentTickets,
            summaries: currentSummaries,
          });
          controller.enqueue(
            new TextEncoder().encode(`data: ${data}\n\n`)
          );
        },
        cancel(controller) {
          sseClients.delete(controller);
        },
      });
      return new Response(stream, {
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache",
          Connection: "keep-alive",
          "Access-Control-Allow-Origin": "*",
        },
      });
    }

    // API: generate continuation prompt for a ticket
    if (url.pathname.startsWith("/api/prompt/")) {
      const ticketId = url.pathname.split("/").pop();
      try {
        const result =
          await $`${RS_HACK} arch tickets --prompt ${ticketId} -p ${WORKSPACE}`.text();
        return new Response(result, {
          headers: { "Content-Type": "text/markdown" },
        });
      } catch (e: any) {
        return Response.json(
          { error: `Ticket '${ticketId}' not found` },
          { status: 404 }
        );
      }
    }

    // API: generate relay doc for a ticket
    if (url.pathname.startsWith("/api/relay-doc/")) {
      const ticketId = url.pathname.split("/").pop();
      try {
        const result =
          await $`${RS_HACK} arch tickets --relay-doc ${ticketId} -p ${WORKSPACE}`.text();
        return new Response(result, {
          headers: { "Content-Type": "text/markdown" },
        });
      } catch (e: any) {
        return Response.json(
          { error: `Ticket '${ticketId}' not found` },
          { status: 404 }
        );
      }
    }

    // API: promote a summary to a relay ticket
    if (
      url.pathname.startsWith("/api/promote/") &&
      req.method === "POST"
    ) {
      const summaryId = decodeURIComponent(
        url.pathname.split("/").pop() || ""
      );
      const summary = currentSummaries.find((s) => s.id === summaryId);
      if (!summary) {
        return Response.json(
          { error: `Summary '${summaryId}' not found` },
          { status: 404 }
        );
      }

      try {
        // Find next R-number from existing tickets
        const existingRelays = currentTickets
          .filter((t) => t.id.startsWith("R"))
          .map((t) => parseInt(t.id.slice(1)) || 0);
        // Also check summaries that were already promoted
        const promotedRelays = currentSummaries
          .filter((s) => s.promoted)
          .map((s) => {
            const match = (s as any).relay_id?.match(/R(\d+)/);
            return match ? parseInt(match[1]) : 0;
          });
        const nextNum =
          Math.max(0, ...existingRelays, ...promotedRelays) + 1;
        const relayId = `R${String(nextNum).padStart(3, "0")}`;

        const title = summary.text.split("\n")[0].slice(0, 80);

        // Update the summary file in place: add relay_id, set promoted: true
        const summaryPath = join(
          WORKSPACE,
          ".hack",
          "summaries",
          `${summary.id}.md`
        );
        let content = await Bun.file(summaryPath).text();
        content = content.replace("promoted: false", "promoted: true");
        // Add relay_id to frontmatter
        content = content.replace(
          "promoted: true",
          `promoted: true\nrelay_id: ${relayId}\nrelay_title: ${title}`
        );
        await Bun.write(summaryPath, content);

        // Trigger rescan
        triggerRescan("promote");

        return Response.json({
          ok: true,
          relayId,
          summaryFile: summaryPath,
          message: `Promoted to ${relayId}`,
        });
      } catch (e: any) {
        return Response.json(
          { error: `Promote failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: manual nudge
    if (url.pathname === "/api/nudge" && req.method === "POST") {
      triggerRescan("api:nudge");
      return Response.json({ ok: true });
    }

    // API: scan stats
    if (url.pathname === "/api/status") {
      return Response.json({
        workspace: WORKSPACE,
        ticketCount: currentTickets.length,
        summaryCount: currentSummaries.length,
        scanCount,
        sseClients: sseClients.size,
      });
    }

    // Static files
    let filePath = url.pathname === "/" ? "/index.html" : url.pathname;
    const file = Bun.file(join(publicDir, filePath));
    if (await file.exists()) {
      return new Response(file);
    }

    // Try dist/ for built assets
    const distFile = Bun.file(join(publicDir, "dist", filePath));
    if (await distFile.exists()) {
      return new Response(distFile);
    }

    return new Response("Not found", { status: 404 });
  },
});

// ── Initial scan ────────────────────────────────────────────────────────

[currentTickets, currentSummaries] = await Promise.all([
  scanTickets(),
  scanSummaries(),
]);
console.log(
  `[hack-board] http://localhost:${PORT} | ${currentTickets.length} tickets, ${currentSummaries.length} summaries | workspace: ${WORKSPACE}`
);

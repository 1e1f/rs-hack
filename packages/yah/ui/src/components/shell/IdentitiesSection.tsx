import { useCallback, useEffect, useState } from "react";
import { Icon } from "../shared/Glyph";
import { env } from "../../env";
import type {
  WireAuthorization,
  WireIdentity,
  WireProbeOutcome,
  WireProbeReport,
} from "../../env/types";

/* Settings → Identities. Lists every registered SSH-key identity with
   its cross-target authorization status, plus generate / import /
   delete affordances. Probe results land here too — the "Refresh
   probes" button fans out local-files + Hetzner + GitHub via
   identity_probe_all and surfaces each provider's outcome distinctly so
   "no PAT configured" reads differently from "Hetzner returned 401". */

interface ProbeReportState {
  pending: boolean;
  report: WireProbeReport | null;
  error: string | null;
}

export function IdentitiesSection({
  envKind,
}: {
  envKind: "tauri" | "browser" | null;
}) {
  const [identities, setIdentities] = useState<WireIdentity[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [probe, setProbe] = useState<ProbeReportState>({
    pending: false,
    report: null,
    error: null,
  });
  const [creating, setCreating] = useState(false);
  const [importing, setImporting] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const list = await env().rpc.identity.list();
      setIdentities(list);
      setLoadError(null);
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function runProbeAll() {
    setProbe({ pending: true, report: null, error: null });
    try {
      const report = await env().rpc.identity.probeAll();
      setProbe({ pending: false, report, error: null });
      await refresh();
    } catch (err) {
      setProbe({
        pending: false,
        report: null,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  return (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <div className="font-display text-[15px] font-medium text-ink">
          Identities
        </div>
        <button
          onClick={runProbeAll}
          disabled={probe.pending}
          className="flex items-center gap-1 rounded border border-ink-3/45 bg-paper-2 px-2 py-0.5 text-[11px] text-ink-2 hover:bg-vellum/55 disabled:pointer-events-none disabled:opacity-40"
          title="Re-probe local files, Hetzner, and GitHub"
        >
          <Icon name="refresh" size={11} />
          {probe.pending ? "Probing…" : "Refresh probes"}
        </button>
      </div>

      <div className="mb-3 text-[11px] text-ink-3">
        SSH keys yah knows about, plus where each is currently authorized.
        yah-generated keys live in <code className="font-mono">~/.yah/keys/</code>;
        imported keys are referenced by their original path — yah never
        copies private bytes.
      </div>

      {envKind === "browser" && (
        <div className="mb-3 rounded border border-amber-700/30 bg-amber-100/20 px-3 py-2 text-[11px] text-ink-2 dark:border-amber-300/20 dark:bg-amber-300/10">
          <span className="font-medium">Browser preview</span>
          {" — "}
          identity registry not reachable. Run under Tauri to manage real keys.
        </div>
      )}

      {probe.report && <ProbeReportBanner report={probe.report} />}
      {probe.error && (
        <div className="mb-3 rounded border border-oxblood/30 bg-oxblood/5 px-3 py-2 text-[11px] text-oxblood">
          ✗ {probe.error}
        </div>
      )}

      <div className="mb-3 flex items-center gap-1.5">
        <button
          onClick={() => {
            setCreating(true);
            setImporting(false);
          }}
          className="flex items-center gap-1 rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2"
        >
          <Icon name="plus" size={11} />
          Generate
        </button>
        <button
          onClick={() => {
            setImporting(true);
            setCreating(false);
          }}
          className="flex items-center gap-1 rounded border border-ink-3/45 bg-paper-2 px-2 py-1 text-[11px] text-ink-2 hover:bg-vellum/55"
        >
          <Icon name="folder" size={11} />
          Import existing
        </button>
      </div>

      {creating && (
        <GenerateForm
          onCancel={() => setCreating(false)}
          onSaved={async () => {
            setCreating(false);
            await refresh();
          }}
        />
      )}
      {importing && (
        <ImportForm
          onCancel={() => setImporting(false)}
          onSaved={async () => {
            setImporting(false);
            await refresh();
          }}
        />
      )}

      {loadError && (
        <div className="my-3 rounded border border-oxblood/30 bg-oxblood/5 px-3 py-2 text-[11px] text-oxblood">
          ✗ {loadError}
        </div>
      )}

      {identities !== null && identities.length === 0 && (
        <div className="rounded border border-rule/40 bg-vellum/30 px-3 py-3 text-[12px] text-ink-3">
          No identities yet. Generate a yah-managed ed25519 key, or import
          one from <code className="font-mono">~/.ssh/</code>.
        </div>
      )}

      <div className="flex flex-col gap-2">
        {(identities ?? []).map((identity) => (
          <IdentityRow
            key={identity.id}
            identity={identity}
            onChanged={refresh}
          />
        ))}
      </div>
    </div>
  );
}

function ProbeReportBanner({ report }: { report: WireProbeReport }) {
  return (
    <div className="mb-3 rounded border border-rule/40 bg-vellum/40 px-3 py-2 text-[11px] text-ink-2">
      <div className="flex items-center gap-2">
        <Icon name="check" size={11} />
        <span>
          Probed {report.identitiesTotal}{" "}
          {report.identitiesTotal === 1 ? "identity" : "identities"}
          {report.localAdded > 0
            ? `, discovered ${report.localAdded} new local key${report.localAdded === 1 ? "" : "s"}`
            : ""}
          .
        </span>
      </div>
      <div className="mt-1 flex flex-col gap-0.5 pl-[18px] text-ink-3">
        <ProbeOutcomeLine label="Hetzner" outcome={report.hetzner} />
        <ProbeOutcomeLine label="GitHub" outcome={report.github} />
      </div>
    </div>
  );
}

function ProbeOutcomeLine({
  label,
  outcome,
}: {
  label: string;
  outcome: WireProbeOutcome;
}) {
  if (outcome.kind === "ok") {
    return (
      <span>
        <span className="font-medium text-ink-2">{label}</span> — matched{" "}
        {outcome.matches} {outcome.matches === 1 ? "key" : "keys"}.
      </span>
    );
  }
  if (outcome.kind === "skipped") {
    return (
      <span>
        <span className="font-medium text-ink-2">{label}</span> — skipped:{" "}
        {outcome.reason}
      </span>
    );
  }
  return (
    <span className="text-oxblood">
      <span className="font-medium">{label}</span> — error: {outcome.reason}
    </span>
  );
}

function GenerateForm({
  onCancel,
  onSaved,
}: {
  onCancel: () => void;
  onSaved: () => void | Promise<void>;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    const trimmed = name.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await env().rpc.identity.create(trimmed);
      await onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
      className="mb-3 rounded border border-rule/50 bg-vellum/40 px-3 py-2.5"
    >
      <div className="mb-1.5 text-[11px] text-ink-3">
        Generates an ed25519 keypair under{" "}
        <code className="font-mono">~/.yah/keys/&lt;name&gt;</code> with
        <code className="font-mono"> 0600</code> permissions. Refuses to
        clobber an existing keyfile.
      </div>
      <div className="flex items-center gap-2">
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. yah-personal"
          spellCheck={false}
          className="min-w-0 flex-1 rounded border border-rule/50 bg-paper-2 px-2 py-1 font-mono text-[11px] text-ink outline-none focus:border-accent/60"
        />
        <button
          type="button"
          onClick={onCancel}
          className="rounded px-2 py-1 text-[11px] text-ink-2 hover:bg-vellum/55"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={!name.trim() || busy}
          className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
        >
          {busy ? "Generating…" : "Generate"}
        </button>
      </div>
      {error && (
        <div className="mt-2 text-[11px] text-oxblood">✗ {error}</div>
      )}
    </form>
  );
}

function ImportForm({
  onCancel,
  onSaved,
}: {
  onCancel: () => void;
  onSaved: () => void | Promise<void>;
}) {
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    const trimmed = path.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await env().rpc.identity.import(trimmed, name.trim() || undefined);
      await onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
      className="mb-3 rounded border border-rule/50 bg-vellum/40 px-3 py-2.5"
    >
      <div className="mb-1.5 text-[11px] text-ink-3">
        Reference an existing public-key file (typically{" "}
        <code className="font-mono">~/.ssh/id_*.pub</code>). yah never reads
        or copies the private half.
      </div>
      <div className="flex flex-col gap-1.5">
        <input
          autoFocus
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="Absolute path to public key (e.g. /Users/leif/.ssh/id_ed25519.pub)"
          spellCheck={false}
          className="rounded border border-rule/50 bg-paper-2 px-2 py-1 font-mono text-[11px] text-ink outline-none focus:border-accent/60"
        />
        <div className="flex items-center gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Optional display name (defaults to filename)"
            spellCheck={false}
            className="min-w-0 flex-1 rounded border border-rule/50 bg-paper-2 px-2 py-1 font-mono text-[11px] text-ink outline-none focus:border-accent/60"
          />
          <button
            type="button"
            onClick={onCancel}
            className="rounded px-2 py-1 text-[11px] text-ink-2 hover:bg-vellum/55"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!path.trim() || busy}
            className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
          >
            {busy ? "Importing…" : "Import"}
          </button>
        </div>
      </div>
      {error && (
        <div className="mt-2 text-[11px] text-oxblood">✗ {error}</div>
      )}
    </form>
  );
}

function IdentityRow({
  identity,
  onChanged,
}: {
  identity: WireIdentity;
  onChanged: () => void | Promise<void>;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function remove() {
    setBusy(true);
    setError(null);
    try {
      await env().rpc.identity.remove(identity.id);
      await onChanged();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  const sourceLabel =
    identity.source.kind === "yahGenerated"
      ? "yah-managed"
      : "imported";
  const sourcePath =
    identity.source.kind === "yahGenerated"
      ? identity.source.privateKeyPath
      : identity.source.publicKeyPath;

  return (
    <div className="rounded border border-rule/40 bg-vellum/40 px-3 py-2.5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-medium text-ink">
              {identity.name}
            </span>
            <span className="rounded bg-accent/15 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-ink-2">
              {sourceLabel}
            </span>
            <span className="text-[10px] text-ink-3">{identity.algorithm}</span>
          </div>
          <div
            className="mt-0.5 truncate font-mono text-[10px] text-ink-3"
            title={identity.id}
          >
            {identity.id}
          </div>
          <div
            className="mt-0.5 truncate text-[10px] text-ink-3/80"
            title={sourcePath}
          >
            {sourcePath}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {!confirmDelete && (
            <button
              onClick={() => setConfirmDelete(true)}
              disabled={busy}
              className="rounded border border-ink-3/45 bg-paper-2 px-[5px] py-0.5 text-[11px] text-ink-3 hover:bg-vellum/55 hover:text-oxblood disabled:opacity-40"
            >
              Delete
            </button>
          )}
          {confirmDelete && (
            <>
              <button
                onClick={() => setConfirmDelete(false)}
                disabled={busy}
                className="rounded border border-ink-3/45 bg-paper-2 px-[5px] py-0.5 text-[11px] text-ink-3 hover:bg-vellum/55"
              >
                Cancel
              </button>
              <button
                onClick={remove}
                disabled={busy}
                className="rounded bg-oxblood px-2 py-1 text-[11px] font-medium text-paper-2 hover:opacity-90 disabled:pointer-events-none disabled:opacity-40"
              >
                {busy ? "Deleting…" : confirmKeyfileLabel(identity)}
              </button>
            </>
          )}
        </div>
      </div>
      <AuthorizationsList identity={identity} />
      {error && (
        <div className="mt-2 text-[11px] text-oxblood">✗ {error}</div>
      )}
    </div>
  );
}

function confirmKeyfileLabel(identity: WireIdentity): string {
  return identity.source.kind === "yahGenerated"
    ? "Confirm + delete keyfile"
    : "Confirm";
}

function AuthorizationsList({ identity }: { identity: WireIdentity }) {
  if (identity.authorizedAt.length === 0) {
    return (
      <div className="mt-2 text-[11px] text-ink-3/80">
        Not registered at any target yet. Run <em>Refresh probes</em> after
        configuring Hetzner / GitHub tokens, or use the rig card to
        authorize.
      </div>
    );
  }
  return (
    <ul className="mt-2 flex flex-col gap-0.5 text-[11px] text-ink-2">
      {identity.authorizedAt.map((auth, i) => (
        <li key={`${auth.kind}:${i}`} className="flex items-center gap-1.5">
          <Icon name="check" size={10} />
          <span>{describeAuthorization(auth)}</span>
        </li>
      ))}
    </ul>
  );
}

function describeAuthorization(auth: WireAuthorization): string {
  const seen = formatRelative(auth.lastSeen);
  switch (auth.kind) {
    case "hetzner":
      return `Hetzner — project ${auth.projectId} · last seen ${seen}`;
    case "github":
      return `GitHub — ${auth.account} · last seen ${seen}`;
    case "gitlab":
      return `GitLab (${auth.instance}) — ${auth.account} · last seen ${seen}`;
    case "sshHost":
      return `SSH host ${auth.userAtHost} · last seen ${seen}`;
  }
}

function formatRelative(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 0) return "just now";
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

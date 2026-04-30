import { useEffect, useMemo, useState } from "react";
import { getEnv } from "../../env";
import type {
  HetznerCreateServerSpec,
  HetznerImage,
  HetznerLocation,
  HetznerServerType,
  HetznerSshKey,
  LocalSshKey,
} from "../../env/types";

/* New-server form. v2 (was hardcoded; the hardcoded matrix went stale
   across architecture lines and 422'd quietly on submit). The form
   fetches the live catalogue on mount — server_types, locations,
   images — so dropdowns mirror exactly what the operator's project
   can build today.

   The selected location filters server types via each type's
   `prices[].location` array; the selected server type's architecture
   filters images by matching architecture. Names are deduped on the
   image dropdown because Hetzner publishes one record per
   (name, architecture) and `POST /v1/servers` matches by name and
   auto-picks the architecture variant. */

const PREFERRED_LOCATION = "fsn1";
const PREFERRED_IMAGE_FLAVORS = ["debian", "ubuntu", "fedora"];

type SubmitState =
  | { kind: "idle" }
  | { kind: "submitting" }
  | { kind: "error"; message: string };

interface ProvisionFormProps {
  onCreated: () => void;
}

interface CatalogueState {
  serverTypes: HetznerServerType[];
  locations: HetznerLocation[];
  images: HetznerImage[];
}

type CatalogueLoad =
  | { kind: "loading" }
  | { kind: "ok"; catalogue: CatalogueState }
  | { kind: "error"; message: string };

/* Composite SSH-key picker entry. We surface three sources in one
   dropdown:
   - `hetzner` keys are already in the project; their id flows straight
     into the create-server body.
   - `local` keys live in `~/.ssh/`; if picked they get uploaded on
     submit (using the file stem as the Hetzner key name) and the
     resulting id flows into the body.
   - `generate` is a sentinel that triggers the inline "Generate yah
     key" flow before the dropdown is committed.
   We dedupe local entries against Hetzner keys by fingerprint so an
   already-uploaded key only shows up once. */
type SshKeyOption =
  | { kind: "hetzner"; key: HetznerSshKey }
  | { kind: "local"; key: LocalSshKey }
  | { kind: "generate" };

export function ProvisionForm({ onCreated }: ProvisionFormProps) {
  const [catalogue, setCatalogue] = useState<CatalogueLoad>({ kind: "loading" });

  const [name, setName] = useState("");
  const [location, setLocation] = useState<string>("");
  const [serverType, setServerType] = useState<string>("");
  const [image, setImage] = useState<string>("");

  const [sshOptions, setSshOptions] = useState<SshKeyOption[]>([]);
  /* Hetzner accepts multiple keys per server — they all land in the new
     server's authorized_keys at first boot. We track the picker as a
     Set of stable option ids; resolveSshKeyIds turns it into the
     `ssh_keys: number[]` array on submit (uploading any local picks
     just-in-time). Empty set ⇒ no keys, server boots with the root
     password emailed by Hetzner. */
  const [selectedKeyIds, setSelectedKeyIds] = useState<Set<string>>(new Set());
  const [keysLoaded, setKeysLoaded] = useState(false);

  const [showGenerate, setShowGenerate] = useState(false);
  const [genName, setGenName] = useState("yah");
  const [genState, setGenState] = useState<SubmitState>({ kind: "idle" });

  const [submit, setSubmit] = useState<SubmitState>({ kind: "idle" });

  /* One-shot catalogue fetch. Failures here lock the whole form behind
     an error pane — there's nothing meaningful to provision without it. */
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const env = await getEnv();
        const [serverTypes, locations, images] = await Promise.all([
          env.rpc.hetzner.listServerTypes(),
          env.rpc.hetzner.listLocations(),
          env.rpc.hetzner.listImages(),
        ]);
        if (cancelled) return;
        setCatalogue({
          kind: "ok",
          catalogue: { serverTypes, locations, images },
        });
        const initialLocation = pickInitialLocation(locations);
        setLocation(initialLocation);
        const initialType = pickInitialType(serverTypes, initialLocation);
        setServerType(initialType?.name ?? "");
        const initialImage = pickInitialImage(images, initialType?.architecture);
        setImage(initialImage);
      } catch (err) {
        if (!cancelled) {
          setCatalogue({
            kind: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  /* SSH-key fetch. Failures don't block submit — provisioning without
     a key just produces a server with password auth disabled and the
     root password emailed. */
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const env = await getEnv();
        const [hetzner, local] = await Promise.all([
          env.rpc.hetzner.listSshKeys().catch(() => [] as HetznerSshKey[]),
          env.rpc.ssh.listLocal().catch(() => [] as LocalSshKey[]),
        ]);
        if (cancelled) return;
        setSshOptions(buildSshOptions(hetzner, local));
        setKeysLoaded(true);
      } catch {
        if (!cancelled) setKeysLoaded(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const ok = catalogue.kind === "ok" ? catalogue.catalogue : null;

  /* Type narrows to whatever Hetzner builds in the chosen DC. The
     prices array on each type carries one entry per location it's
     deployable in; absence == not buildable. Deprecated SKUs are
     dropped so retired sizes don't linger in the picker. */
  const availableTypes = useMemo(() => {
    if (!ok || !location) return [];
    return ok.serverTypes
      .filter((t) => !t.deprecated)
      .filter((t) => t.prices.some((p) => p.location === location))
      .sort(byPriceAtLocation(location));
  }, [ok, location]);

  const selectedType = availableTypes.find((t) => t.name === serverType);
  const selectedTypePrice = selectedType
    ? selectedType.prices.find((p) => p.location === location)
    : undefined;

  /* Reset type when location flips and the active type isn't sold there. */
  useEffect(() => {
    if (!availableTypes.length) return;
    if (!availableTypes.some((t) => t.name === serverType)) {
      setServerType(availableTypes[0].name);
    }
  }, [availableTypes]);

  /* Images are filtered by architecture so a cax/ARM type never gets
     paired with an x86-only image (which 422s upstream). Deduped by
     name because each (name, architecture) is its own record. */
  const availableImages = useMemo(() => {
    if (!ok || !selectedType) return [];
    const seen = new Set<string>();
    const out: HetznerImage[] = [];
    for (const img of ok.images) {
      if (img.deprecated) continue;
      if (img.architecture !== selectedType.architecture) continue;
      if (seen.has(img.name)) continue;
      seen.add(img.name);
      out.push(img);
    }
    return out.sort(byImagePreference);
  }, [ok, selectedType]);

  /* Reset image when its architecture stops matching the chosen type. */
  useEffect(() => {
    if (!availableImages.length) return;
    if (!availableImages.some((i) => i.name === image)) {
      setImage(availableImages[0].name);
    }
  }, [availableImages]);

  async function refreshKeys(addFingerprint?: string) {
    const env = await getEnv();
    const [hetzner, local] = await Promise.all([
      env.rpc.hetzner.listSshKeys().catch(() => [] as HetznerSshKey[]),
      env.rpc.ssh.listLocal().catch(() => [] as LocalSshKey[]),
    ]);
    const opts = buildSshOptions(hetzner, local);
    setSshOptions(opts);
    if (addFingerprint) {
      const match = opts.find(
        (o) =>
          (o.kind === "hetzner" && o.key.fingerprint === addFingerprint) ||
          (o.kind === "local" && o.key.fingerprint === addFingerprint),
      );
      if (match) {
        setSelectedKeyIds((prev) => {
          const next = new Set(prev);
          next.add(optionId(match));
          return next;
        });
      }
    }
  }

  async function onGenerate() {
    setGenState({ kind: "submitting" });
    try {
      const env = await getEnv();
      const local = await env.rpc.ssh.generate(genName);
      await env.rpc.hetzner.uploadSshKey(local.name, local.public_key);
      await refreshKeys(local.fingerprint);
      setGenState({ kind: "idle" });
      setShowGenerate(false);
    } catch (err) {
      setGenState({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  function toggleKey(id: string) {
    setSelectedKeyIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) {
      setSubmit({ kind: "error", message: "Name is required." });
      return;
    }
    if (!serverType || !location || !image) {
      setSubmit({ kind: "error", message: "Catalogue not loaded yet." });
      return;
    }
    setSubmit({ kind: "submitting" });
    try {
      const env = await getEnv();
      const sshKeyIds = await resolveSshKeyIds(env, selectedKeyIds, sshOptions);
      const spec: HetznerCreateServerSpec = {
        name: name.trim(),
        server_type: serverType,
        location,
        image,
        ssh_keys: sshKeyIds,
      };
      await env.rpc.hetzner.createServer(spec);
      onCreated();
    } catch (err) {
      setSubmit({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  if (catalogue.kind === "loading") {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-ink-3">
        Loading Hetzner catalogue…
      </div>
    );
  }

  if (catalogue.kind === "error") {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <div className="max-w-md text-center text-[13px] text-oxblood">
          Couldn't load Hetzner catalogue: {catalogue.message}
        </div>
      </div>
    );
  }

  const submitting = submit.kind === "submitting";
  const hetznerKeys = sshOptions.filter(
    (o): o is { kind: "hetzner"; key: HetznerSshKey } => o.kind === "hetzner",
  );
  const localKeys = sshOptions.filter(
    (o): o is { kind: "local"; key: LocalSshKey } => o.kind === "local",
  );

  return (
    <form
      onSubmit={onSubmit}
      className="mx-auto flex max-w-2xl flex-col gap-4 px-6 py-6"
    >
      <header className="flex items-baseline justify-between">
        <div className="font-display text-[18px] text-ink-2 [font-variant-caps:all-small-caps]">
          Provision a server
        </div>
        {selectedTypePrice && (
          <div className="text-[12px] text-ink-3">
            ≈ {formatEur(selectedTypePrice.price_monthly_net)}/mo
            <span className="text-ink-4">
              {" "}({formatEur(selectedTypePrice.price_monthly_gross)} incl. VAT)
            </span>
          </div>
        )}
      </header>

      <Field label="Name">
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-server"
          className="w-full rounded border border-line bg-paper px-2 py-1 text-[13px] text-ink focus:border-accent focus:outline-none"
        />
      </Field>

      <Field label="Location">
        <select
          value={location}
          onChange={(e) => setLocation(e.target.value)}
          className="w-full rounded border border-line bg-paper px-2 py-1 text-[13px] text-ink"
        >
          {ok!.locations.map((l) => (
            <option key={l.id} value={l.name}>
              {l.city}, {l.country} · {l.description} ({l.name})
            </option>
          ))}
        </select>
      </Field>

      <Field
        label="Type"
        hint={selectedType ? typeSpec(selectedType) : undefined}
      >
        <select
          value={serverType}
          onChange={(e) => setServerType(e.target.value)}
          disabled={availableTypes.length === 0}
          className="w-full rounded border border-line bg-paper px-2 py-1 text-[13px] text-ink"
        >
          {availableTypes.length === 0 && <option value="">No types in this location</option>}
          {availableTypes.map((t) => {
            const price = t.prices.find((p) => p.location === location);
            return (
              <option key={t.id} value={t.name}>
                {t.name} · {t.architecture.toUpperCase()} · {typeSpec(t)}
                {price && ` — ${formatEur(price.price_monthly_net)}/mo`}
              </option>
            );
          })}
        </select>
      </Field>

      <Field
        label="Image"
        hint={
          selectedType
            ? `${selectedType.architecture}-compatible images only`
            : undefined
        }
      >
        <select
          value={image}
          onChange={(e) => setImage(e.target.value)}
          disabled={availableImages.length === 0}
          className="w-full rounded border border-line bg-paper px-2 py-1 text-[13px] text-ink"
        >
          {availableImages.length === 0 && <option value="">No images for this architecture</option>}
          {availableImages.map((i) => (
            <option key={i.id} value={i.name}>
              {i.description} ({i.name})
            </option>
          ))}
        </select>
      </Field>

      <Field
        label={`SSH keys${selectedKeyIds.size > 0 ? ` (${selectedKeyIds.size} selected)` : ""}`}
        hint={
          keysLoaded
            ? selectedKeyIds.size === 0
              ? "none — root password emailed"
              : undefined
            : "loading keys…"
        }
      >
        <div className="rounded border border-line bg-paper">
          {hetznerKeys.length === 0 && localKeys.length === 0 && keysLoaded && (
            <div className="px-3 py-2 text-[12px] text-ink-3">
              No keys yet. Generate one below or add a token-bearing key in Settings.
            </div>
          )}
          {hetznerKeys.length > 0 && (
            <KeyGroup label="Hetzner project keys">
              {hetznerKeys.map((o) => {
                const id = optionId(o);
                return (
                  <KeyRow
                    key={id}
                    checked={selectedKeyIds.has(id)}
                    onToggle={() => toggleKey(id)}
                    label={o.key.name}
                    hint={shortFp(o.key.fingerprint)}
                  />
                );
              })}
            </KeyGroup>
          )}
          {localKeys.length > 0 && (
            <KeyGroup label="Local keys (uploaded on submit)">
              {localKeys.map((o) => {
                const id = optionId(o);
                return (
                  <KeyRow
                    key={id}
                    checked={selectedKeyIds.has(id)}
                    onToggle={() => toggleKey(id)}
                    label={o.key.name}
                    hint={`${shortFp(o.key.fingerprint)} · ${o.key.algorithm}`}
                  />
                );
              })}
            </KeyGroup>
          )}
          <div className="border-t border-line px-3 py-2">
            {!showGenerate ? (
              <button
                type="button"
                onClick={() => setShowGenerate(true)}
                className="text-[12px] text-accent hover:text-accent-2"
              >
                + Generate new yah key…
              </button>
            ) : (
              <div className="flex items-end gap-2">
                <Field label="Key name" inline>
                  <input
                    type="text"
                    value={genName}
                    onChange={(e) => setGenName(e.target.value)}
                    className="rounded border border-line bg-paper px-2 py-1 text-[13px] text-ink focus:border-accent focus:outline-none"
                  />
                </Field>
                <button
                  type="button"
                  onClick={onGenerate}
                  disabled={genState.kind === "submitting"}
                  className="rounded bg-accent px-3 py-1.5 text-[12px] font-medium text-paper-2 hover:bg-accent-2 disabled:opacity-50"
                >
                  {genState.kind === "submitting" ? "Generating…" : "Generate + upload"}
                </button>
                <button
                  type="button"
                  onClick={() => setShowGenerate(false)}
                  disabled={genState.kind === "submitting"}
                  className="text-[12px] text-ink-3 hover:text-ink-2"
                >
                  Cancel
                </button>
                {genState.kind === "error" && (
                  <div className="text-[12px] text-oxblood">{genState.message}</div>
                )}
              </div>
            )}
          </div>
        </div>
      </Field>

      {submit.kind === "error" && (
        <div className="rounded border border-oxblood/40 bg-oxblood/10 px-3 py-2 text-[12px] text-oxblood">
          {submit.message}
        </div>
      )}

      <div className="flex items-center justify-end gap-2 pt-2">
        <button
          type="submit"
          disabled={submitting || !serverType || !image}
          className="rounded bg-accent px-4 py-1.5 text-[13px] font-medium text-paper-2 hover:bg-accent-2 disabled:opacity-50"
        >
          {submitting ? "Provisioning…" : "Create server"}
        </button>
      </div>
    </form>
  );
}

function KeyGroup({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-line last:border-b-0">
      <div className="px-3 pt-2 pb-1 text-[10px] font-medium uppercase tracking-wide text-ink-4">
        {label}
      </div>
      <div>{children}</div>
    </div>
  );
}

function KeyRow({
  checked,
  onToggle,
  label,
  hint,
}: {
  checked: boolean;
  onToggle: () => void;
  label: string;
  hint: string;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-[13px] hover:bg-paper-2/40">
      <input
        type="checkbox"
        checked={checked}
        onChange={onToggle}
        className="accent-accent"
      />
      <span className="text-ink">{label}</span>
      <span className="font-mono text-[11px] text-ink-3">{hint}</span>
    </label>
  );
}

function Field({
  label,
  hint,
  inline,
  children,
}: {
  label: string;
  hint?: string;
  inline?: boolean;
  children: React.ReactNode;
}) {
  return (
    <label className={inline ? "flex flex-col gap-1" : "flex flex-col gap-1.5"}>
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-3">
        {label}
        {hint && <span className="ml-2 lowercase text-ink-4">{hint}</span>}
      </span>
      {children}
    </label>
  );
}

function typeSpec(t: HetznerServerType): string {
  return `${t.cores} vCPU · ${t.memory} GB · ${t.disk} GB`;
}

function pickInitialLocation(locations: HetznerLocation[]): string {
  if (locations.some((l) => l.name === PREFERRED_LOCATION)) return PREFERRED_LOCATION;
  return locations[0]?.name ?? "";
}

function pickInitialType(
  types: HetznerServerType[],
  location: string,
): HetznerServerType | undefined {
  const live = types
    .filter((t) => !t.deprecated)
    .filter((t) => t.prices.some((p) => p.location === location));
  /* Cheapest x86 SKU at the location is the friendliest default —
     matches the implicit "starter" pick on the Hetzner web console. */
  const sorted = live.slice().sort(byPriceAtLocation(location));
  return sorted[0];
}

function pickInitialImage(images: HetznerImage[], architecture?: string): string {
  if (!architecture) return "";
  const compatible = images.filter(
    (i) => !i.deprecated && i.architecture === architecture,
  );
  const seen = new Set<string>();
  const dedup: HetznerImage[] = [];
  for (const i of compatible) {
    if (seen.has(i.name)) continue;
    seen.add(i.name);
    dedup.push(i);
  }
  dedup.sort(byImagePreference);
  return dedup[0]?.name ?? "";
}

function byPriceAtLocation(location: string) {
  return (a: HetznerServerType, b: HetznerServerType) => {
    const pa = a.prices.find((p) => p.location === location);
    const pb = b.prices.find((p) => p.location === location);
    const na = pa ? Number(pa.price_monthly_net) : Number.POSITIVE_INFINITY;
    const nb = pb ? Number(pb.price_monthly_net) : Number.POSITIVE_INFINITY;
    return na - nb;
  };
}

function byImagePreference(a: HetznerImage, b: HetznerImage): number {
  /* Debian / Ubuntu / Fedora float to the top because they're the
     defaults on the Hetzner console and the only ones most operators
     have ever booted. Everything else falls through to alpha order
     by description. */
  const ai = PREFERRED_IMAGE_FLAVORS.indexOf(a.os_flavor);
  const bi = PREFERRED_IMAGE_FLAVORS.indexOf(b.os_flavor);
  const ar = ai === -1 ? PREFERRED_IMAGE_FLAVORS.length : ai;
  const br = bi === -1 ? PREFERRED_IMAGE_FLAVORS.length : bi;
  if (ar !== br) return ar - br;
  /* Newer OS version first when same flavor. */
  return b.description.localeCompare(a.description);
}

function buildSshOptions(
  hetzner: HetznerSshKey[],
  local: LocalSshKey[],
): SshKeyOption[] {
  const knownFingerprints = new Set(hetzner.map((k) => k.fingerprint));
  const out: SshKeyOption[] = [];
  for (const k of hetzner) out.push({ kind: "hetzner", key: k });
  for (const k of local) {
    if (knownFingerprints.has(k.fingerprint)) continue;
    out.push({ kind: "local", key: k });
  }
  return out;
}

function optionId(o: SshKeyOption): string {
  if (o.kind === "hetzner") return `hetzner:${o.key.id}`;
  if (o.kind === "local") return `local:${o.key.fingerprint}`;
  return "__generate__";
}

/* Walk the checked options, uploading any local picks just-in-time so
   the create-server body carries Hetzner-side ids. Order is preserved
   from the dropdown so the caller's list matches what they ticked. */
async function resolveSshKeyIds(
  env: Awaited<ReturnType<typeof getEnv>>,
  selectedKeyIds: Set<string>,
  options: SshKeyOption[],
): Promise<number[]> {
  const out: number[] = [];
  for (const opt of options) {
    const id = optionId(opt);
    if (!selectedKeyIds.has(id)) continue;
    if (opt.kind === "hetzner") {
      out.push(opt.key.id);
    } else if (opt.kind === "local") {
      const uploaded = await env.rpc.hetzner.uploadSshKey(opt.key.name, opt.key.public_key);
      out.push(uploaded.id);
    }
  }
  return out;
}

function shortFp(fp: string): string {
  const colon = fp.indexOf(":");
  const tail = colon === -1 ? fp : fp.slice(colon + 1);
  return `…${tail.slice(-12)}`;
}

/* Hetzner returns prices as decimal strings with up to 10 fraction
   digits ("3.7900000000"). The form is operator-facing — fixed two
   decimals matches the convention on hetzner.com/cloud and avoids
   noise like "€3.7900000000/mo". */
function formatEur(s: string): string {
  const n = Number(s);
  if (!Number.isFinite(n)) return `€${s}`;
  return `€${n.toFixed(2)}`;
}

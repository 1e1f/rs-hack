//! @yah:ticket(R034-F5, "identity ranking helper — picker auto-select")
//! @yah:assignee(agent:claude)
//! @yah:status(in-progress)
//! @yah:phase(P4)
//! @yah:parent(R034)
//! @arch:see(architecture/yah-identities.md)

import type { WireAuthorization, WireIdentity } from "../env/types";

/* Structural matcher for "this identity covers <target>." Each variant
   is a partial — fields left undefined are wildcards (e.g. a Hetzner
   target with no projectId matches any Hetzner authorization). The
   shape mirrors `WireAuthorization` so callers can express required
   coverage without inventing a parallel vocabulary. */
export type IdentityTarget =
  | { kind: "hetzner"; projectId?: string }
  | { kind: "github"; account?: string }
  | { kind: "gitlab"; instance?: string; account?: string }
  | { kind: "sshHost"; userAtHost?: string };

export interface RankedIdentity {
  identity: WireIdentity;
  /** Number of `targets` that this identity already covers. */
  coveredCount: number;
  /** Tier per the spec in yah-identities.md §UX:
   *  1 = covers all required targets,
   *  2 = covers some,
   *  3 = covers none, yah-generated (greenfield bias),
   *  4 = covers none, imported.
   *  Lower is better. */
  tier: 1 | 2 | 3 | 4;
}

function matches(auth: WireAuthorization, target: IdentityTarget): boolean {
  if (auth.kind !== target.kind) return false;
  switch (target.kind) {
    case "hetzner":
      return target.projectId === undefined || target.projectId === (auth as Extract<WireAuthorization, { kind: "hetzner" }>).projectId;
    case "github":
      return target.account === undefined || target.account === (auth as Extract<WireAuthorization, { kind: "github" }>).account;
    case "gitlab": {
      const a = auth as Extract<WireAuthorization, { kind: "gitlab" }>;
      return (
        (target.instance === undefined || target.instance === a.instance) &&
        (target.account === undefined || target.account === a.account)
      );
    }
    case "sshHost":
      return target.userAtHost === undefined || target.userAtHost === (auth as Extract<WireAuthorization, { kind: "sshHost" }>).userAtHost;
  }
}

export function coversTarget(identity: WireIdentity, target: IdentityTarget): boolean {
  return identity.authorizedAt.some((auth) => matches(auth, target));
}

/* Rank a set of identities against the targets a rig action needs. The
   ordering follows yah-identities.md §"UX: picking the right identity":
   tier ascending, then coveredCount descending (tier 2 only), then
   last_used_at descending. yah-generated keys win the tier-3/4
   tie-break so a greenfield user gets nudged toward the registry's
   own keys. */
export function rankIdentities(
  identities: WireIdentity[],
  targets: IdentityTarget[],
): RankedIdentity[] {
  const ranked: RankedIdentity[] = identities.map((identity) => {
    const coveredCount = targets.filter((t) => coversTarget(identity, t)).length;
    let tier: RankedIdentity["tier"];
    if (targets.length > 0 && coveredCount === targets.length) tier = 1;
    else if (coveredCount > 0) tier = 2;
    else if (identity.source.kind === "yahGenerated") tier = 3;
    else tier = 4;
    return { identity, coveredCount, tier };
  });

  ranked.sort((a, b) => {
    if (a.tier !== b.tier) return a.tier - b.tier;
    if (a.tier === 2 && a.coveredCount !== b.coveredCount) {
      return b.coveredCount - a.coveredCount;
    }
    const aLast = a.identity.lastUsedAt ?? 0;
    const bLast = b.identity.lastUsedAt ?? 0;
    if (aLast !== bLast) return bLast - aLast;
    return a.identity.name.localeCompare(b.identity.name);
  });

  return ranked;
}

/* Inverse of `privateKeyPathForIdentity`: given the on-disk key path a
   rig was attached with, find the identity that points at the same
   bytes. Used by the rig selector to label a remote rig with its
   bound identity (via R034-T6's full migration, this lookup gets
   replaced by `rig.identityId`, but until then keyPath is what we
   have). */
export function findIdentityByKeyPath(
  identities: WireIdentity[],
  keyPath: string | null | undefined,
): WireIdentity | null {
  if (!keyPath) return null;
  return identities.find((i) => privateKeyPathForIdentity(i) === keyPath) ?? null;
}

/* Pull the on-disk path the daemon should use when authenticating with
   this identity. yah-generated → the managed private key path. Imported
   → the user-supplied private key when present, else the public-key
   path with the `.pub` suffix stripped (mirrors the convention used in
   ConnectRemoteRigModal.resolveKeyForServer). Returns null when nothing
   usable is on record — the caller surfaces a "private half not
   reachable" hint rather than guessing. */
export function privateKeyPathForIdentity(identity: WireIdentity): string | null {
  if (identity.source.kind === "yahGenerated") {
    return identity.source.privateKeyPath;
  }
  if (identity.source.privateKeyPath) return identity.source.privateKeyPath;
  return identity.source.publicKeyPath.replace(/\.pub$/, "");
}

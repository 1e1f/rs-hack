import { expect, test } from "bun:test";
import {
  coversTarget,
  privateKeyPathForIdentity,
  rankIdentities,
  type IdentityTarget,
} from "./identity-ranking";
import type { WireIdentity } from "../env/types";

function id(
  name: string,
  source: WireIdentity["source"],
  authorizedAt: WireIdentity["authorizedAt"] = [],
  lastUsedAt: number | null = null,
): WireIdentity {
  return {
    id: `SHA256:${name}`,
    name,
    algorithm: "ssh-ed25519",
    publicKey: `ssh-ed25519 AAAA...${name} ${name}@laptop`,
    source,
    authorizedAt,
    createdAt: 0,
    lastUsedAt,
  };
}

const yahGen = (name: string): WireIdentity["source"] => ({
  kind: "yahGenerated",
  privateKeyPath: `/home/me/.yah/keys/${name}`,
});

const imported = (name: string): WireIdentity["source"] => ({
  kind: "imported",
  publicKeyPath: `/home/me/.ssh/${name}.pub`,
  privateKeyPath: `/home/me/.ssh/${name}`,
});

test("coversTarget — Hetzner project match honors projectId", () => {
  const k = id("k", yahGen("k"), [
    { kind: "hetzner", projectId: "default", keyIdInHetzner: 1, name: "k", lastSeen: 0 },
  ]);
  expect(coversTarget(k, { kind: "hetzner" })).toBe(true);
  expect(coversTarget(k, { kind: "hetzner", projectId: "default" })).toBe(true);
  expect(coversTarget(k, { kind: "hetzner", projectId: "staging" })).toBe(false);
});

test("rankIdentities — tier 1 wins over tier 2 even with higher last_used_at", () => {
  const targets: IdentityTarget[] = [{ kind: "hetzner" }, { kind: "github" }];
  const all = id(
    "all",
    yahGen("all"),
    [
      { kind: "hetzner", projectId: "p", keyIdInHetzner: 1, name: "all", lastSeen: 0 },
      { kind: "github", account: "leif", keyId: 2, title: "all", lastSeen: 0 },
    ],
    100, // older
  );
  const some = id(
    "some",
    yahGen("some"),
    [{ kind: "hetzner", projectId: "p", keyIdInHetzner: 3, name: "some", lastSeen: 0 }],
    9999, // newer
  );
  const out = rankIdentities([some, all], targets);
  expect(out[0].identity.name).toBe("all");
  expect(out[0].tier).toBe(1);
  expect(out[1].identity.name).toBe("some");
  expect(out[1].tier).toBe(2);
});

test("rankIdentities — tier 3 (yah-generated greenfield) beats tier 4 (imported)", () => {
  const fresh = id("fresh", yahGen("fresh"));
  const old = id("old", imported("old"));
  const out = rankIdentities([old, fresh], [{ kind: "hetzner" }]);
  expect(out[0].identity.name).toBe("fresh");
  expect(out[0].tier).toBe(3);
  expect(out[1].tier).toBe(4);
});

test("rankIdentities — empty targets ⇒ tier 3/4 by source, recency tie-break", () => {
  const a = id("a", yahGen("a"), [], 1);
  const b = id("b", yahGen("b"), [], 99);
  const c = id("c", imported("c"), [], 999);
  const out = rankIdentities([a, b, c], []);
  expect(out.map((r) => r.identity.name)).toEqual(["b", "a", "c"]);
});

test("rankIdentities — tier 2 sorts by coveredCount descending", () => {
  const targets: IdentityTarget[] = [
    { kind: "hetzner" },
    { kind: "github" },
    { kind: "gitlab" },
  ];
  const one = id("one", yahGen("one"), [
    { kind: "hetzner", projectId: "p", keyIdInHetzner: 1, name: "one", lastSeen: 0 },
  ]);
  const two = id("two", yahGen("two"), [
    { kind: "hetzner", projectId: "p", keyIdInHetzner: 2, name: "two", lastSeen: 0 },
    { kind: "github", account: "leif", keyId: 3, title: "two", lastSeen: 0 },
  ]);
  const out = rankIdentities([one, two], targets);
  expect(out[0].identity.name).toBe("two");
  expect(out[0].coveredCount).toBe(2);
  expect(out[1].coveredCount).toBe(1);
});

test("privateKeyPathForIdentity — strips .pub from imported public path fallback", () => {
  const yah = id("yah", yahGen("yah"));
  expect(privateKeyPathForIdentity(yah)).toBe("/home/me/.yah/keys/yah");

  const imp = id("imp", {
    kind: "imported",
    publicKeyPath: "/home/me/.ssh/id_ed25519.pub",
    privateKeyPath: "/home/me/.ssh/id_ed25519",
  });
  expect(privateKeyPathForIdentity(imp)).toBe("/home/me/.ssh/id_ed25519");

  const noPriv = id("nop", {
    kind: "imported",
    publicKeyPath: "/home/me/.ssh/id_rsa.pub",
  });
  expect(privateKeyPathForIdentity(noPriv)).toBe("/home/me/.ssh/id_rsa");
});

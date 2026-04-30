//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! OS-keychain-backed API token storage for cloud providers and agent
//! engines (Hetzner, Cloudflare, OpenAI, Anthropic, GitHub, future
//! DO/AWS). Backs the Settings → API Keys panel and the Rust-side
//! provider clients that round-trip tokens to upstream APIs.
//!
//! # Storage shape: single-blob vault
//!
//! Per-provider tokens live inside one keychain entry — `(yah, vault)` —
//! whose value is a JSON `{ "<provider>": "<token>" }` map. On the cold
//! load (first read in a process) we pay one Keychain prompt; the result
//! is cached in-process for the rest of the session, so subsequent
//! `get` / `set` / `has` / `delete` calls don't trigger any prompt at
//! all. Per-provider entries from before this layout existed are folded
//! into the vault by [`migrate_legacy_entries`] on app boot and the
//! originals are deleted, so the migration is one-shot.
//!
//! Why one entry instead of N: macOS Keychain prompts are per-item, and
//! a multi-engine agent run can touch four or five providers in a single
//! turn. The single-blob vault collapses that into one prompt per
//! process, which is the difference between "yah is usable" and "yah
//! pesters me every time I send a message." The trade-off is that we
//! lose per-item Keychain ACL granularity — anything authenticated as
//! yah gets every entry in the vault. For yah's solo-dev / single-app
//! threat model this is a non-issue: `api_keys::get` was already a
//! Rust-only call available to anything authenticated as yah, so the
//! ACL surface didn't add a meaningful boundary.
//!
//! Tauri commands exposed to the renderer:
//!
//! * `api_key_set(provider, token)`
//! * `api_key_has(provider) -> bool`
//! * `api_key_delete(provider) -> bool` (true if an entry was removed)
//!
//! `get` is **deliberately Rust-only** (see [`get`]). The threat-model
//! in `architecture/settings-api-keys.md` requires that tokens never
//! reach renderer JS after first set — provider clients (Hetzner reader,
//! GitHub reader, agent engines) call [`get`] from the Tauri host, hit
//! the upstream API, and return only the parsed result to the renderer.
//! The renderer's only credential affordance is `has(provider)` for UI
//! gating.
//!
//! Provider names are validated to ASCII alphanumeric + `-`/`_` so a
//! malformed string can't write under an unexpected vault key. The
//! renderer's `apiKey.set('cloudflare', …)` call passes a fixed
//! allowlisted enum, but we re-validate here as defense-in-depth — a
//! Tauri command is reachable from any renderer code, including future
//! plugin surfaces.
//!
//! # Identity passphrases
//!
//! The identity registry's planned `Lock` affordance
//! (`yah-identities.md` P5) will store per-identity passphrases under
//! the same vault, keyed `identity:<fingerprint>` — no new keychain
//! entry, no second prompt.
//!
//! @yah:ticket(R027-T7, "Single-blob keychain vault: collapse per-provider entries + in-memory cache + boot migration")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R027)
//! @yah:next("Replace per-provider keyring::Entry::new(\"yah\", provider) with a single (\"yah\", \"vault\") entry whose value is JSON: HashMap<String, serde_json::Value>. Reads load + cache; writes read-modify-write.")
//! @yah:next("Add an in-memory cache (Arc<RwLock<HashMap<String, Value>>>) loaded lazily on first access — within one process run, only the cold-load pays the keychain prompt.")
//! @yah:next("Boot-time migration: on first AppState init, walk the known per-provider names (\"hetzner\", \"cloudflare\", \"openai\", \"anthropic\", \"claude_oauth\", whatever's currently set) and fold any hits into the vault, then delete the originals so the cleanup is one-shot. Idempotent — second boot is a no-op.")
//! @yah:next("Public surface unchanged: api_key_set/has/delete Tauri commands keep the same signatures; api_keys::get/set internal helpers stay the same shape so hetzner.rs / agent.rs callers don't move. Identity-passphrase support (yah-identities.md P5 Lock affordance) drops in by reusing the same vault under \"identity:<fingerprint>\" keys.")
//! @yah:next("Threat-model note: lose per-item Keychain ACL granularity. For yah's solo-dev / single-app threat model this is a non-issue — anything that authenticates as yah already gets every per-provider item via the existing api_keys::get path. Document in api_keys.rs module doc.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib api_keys")
//! @yah:verify("On macOS dev build: set 2-3 api keys via Settings UI, restart yah, observe exactly one Keychain prompt for the whole session")
//! @arch:see(architecture/yah-identities.md)
//! @arch:see(architecture/settings-api-keys.md)

use keyring::Entry;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

const KEYRING_SERVICE: &str = "yah";

/// Account name of the single keychain entry that holds the JSON
/// `provider→token` map.
const VAULT_ACCOUNT: &str = "vault";

/// Per-provider entries that may exist from before the single-blob
/// vault landed. The boot migration folds any of these into the vault
/// and deletes the originals so the cleanup is one-shot. Names line up
/// with the constants used in `hetzner.rs`, `identities.rs`, `agent.rs`
/// — adding a new provider only matters here if there are real installs
/// in the wild that wrote a per-provider entry under the old layout.
const LEGACY_PROVIDERS: &[&str] = &[
    "hetzner",
    "cloudflare",
    "openai",
    "anthropic",
    "anthropic-oauth",
    // Older naming the codebase has used for the OAuth slot — keep
    // both spellings in the migration sweep so an install that
    // predates the rename still gets folded in.
    "claude-oauth",
    "claude_oauth",
    "ollama",
    "github",
    "gitlab",
    "digitalocean",
    "aws-s3",
];

#[derive(Debug, thiserror::Error)]
pub enum ApiKeyError {
    #[error("invalid provider name: {0:?} (expected ASCII alphanumeric + -/_)")]
    InvalidProvider(String),
    #[error("keychain access failed: {0}")]
    Keychain(#[from] keyring::Error),
}

pub fn validate_provider(provider: &str) -> Result<(), ApiKeyError> {
    let valid = !provider.is_empty()
        && provider
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(ApiKeyError::InvalidProvider(provider.to_string()))
    }
}

// ---------- In-memory vault cache ----------
//
// `None` = vault has never been loaded in this process; first access
// hits the keychain (one prompt on macOS) and populates the slot.
// `Some(map)` = the canonical in-memory snapshot. All reads/writes
// flow through it so subsequent calls are pure-memory work.

static VAULT_CACHE: OnceLock<RwLock<Option<HashMap<String, String>>>> = OnceLock::new();

fn cache() -> &'static RwLock<Option<HashMap<String, String>>> {
    VAULT_CACHE.get_or_init(|| RwLock::new(None))
}

/// Drop the in-process cache so the next call cold-loads from the
/// keychain. Test-only — production code wants the cache to live for
/// the entire process lifetime, that's the whole point.
#[cfg(test)]
fn reset_cache() {
    *cache().write().unwrap() = None;
}

// ---------- Keychain shim ----------
//
// Thin wrappers around `keyring::Entry` that normalize "no entry" to
// `Ok(None)` / `Ok(())` so the vault and migration layers don't have
// to match on `keyring::Error::NoEntry` everywhere.

fn read_keychain(account: &str) -> Result<Option<String>, ApiKeyError> {
    let entry = Entry::new(KEYRING_SERVICE, account)?;
    match entry.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn write_keychain(account: &str, value: &str) -> Result<(), ApiKeyError> {
    Entry::new(KEYRING_SERVICE, account)?.set_password(value)?;
    Ok(())
}

fn delete_keychain(account: &str) -> Result<(), ApiKeyError> {
    let entry = Entry::new(KEYRING_SERVICE, account)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

// ---------- Vault load/save ----------

/// Permissive parser: a malformed vault blob (someone hand-edited the
/// keychain entry, an older yah wrote an unsupported shape) shouldn't
/// brick the app. Log and treat as empty — the next `set` rewrites the
/// blob from scratch.
fn parse_vault(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).unwrap_or_else(|err| {
        tracing::warn!(error = %err, "vault JSON malformed; treating as empty");
        HashMap::new()
    })
}

fn load_vault() -> Result<HashMap<String, String>, ApiKeyError> {
    {
        let guard = cache().read().unwrap();
        if let Some(m) = guard.as_ref() {
            return Ok(m.clone());
        }
    }
    let mut guard = cache().write().unwrap();
    if let Some(m) = guard.as_ref() {
        return Ok(m.clone());
    }
    let map = match read_keychain(VAULT_ACCOUNT)? {
        Some(s) => parse_vault(&s),
        None => HashMap::new(),
    };
    *guard = Some(map.clone());
    Ok(map)
}

fn save_vault(map: &HashMap<String, String>) -> Result<(), ApiKeyError> {
    if map.is_empty() {
        // Empty map → drop the keychain entry so a fresh `delete` of
        // the last token tidies up after itself rather than leaving an
        // orphan `{}` blob.
        delete_keychain(VAULT_ACCOUNT)?;
    } else {
        let json = serde_json::to_string(map)
            .expect("HashMap<String, String> always serializes");
        write_keychain(VAULT_ACCOUNT, &json)?;
    }
    *cache().write().unwrap() = Some(map.clone());
    Ok(())
}

// ---------- Public API ----------
//
// Same signatures as the previous per-provider-entry version — the
// vault is an internal-shape change, no caller in `hetzner.rs`,
// `identities.rs`, or `agent.rs` needs to move.

/// Write `token` to the vault under `provider`. Overwrites any existing
/// token for the same provider (matches `gh auth login` ergonomics — no
/// two-step delete-then-set required to rotate).
pub fn set(provider: &str, token: &str) -> Result<(), ApiKeyError> {
    validate_provider(provider)?;
    let mut map = load_vault()?;
    map.insert(provider.to_string(), token.to_string());
    save_vault(&map)
}

/// Whether the vault currently holds a token for `provider`. Returns
/// `false` for any error path (malformed provider name, platform
/// failure) — the renderer's UI gating just needs the boolean, and
/// surfacing keychain errors here would force every read site to handle
/// them. Real failures still surface from [`set`] / [`delete`].
pub fn has(provider: &str) -> bool {
    matches!(get(provider), Ok(Some(_)))
}

/// Read the stored token for `provider`. **Rust-only** — never expose
/// this as a Tauri command. Returns `Ok(None)` when no token has been
/// set, so callers can branch cleanly.
pub fn get(provider: &str) -> Result<Option<String>, ApiKeyError> {
    validate_provider(provider)?;
    let map = load_vault()?;
    Ok(map.get(provider).cloned())
}

/// Remove `provider`'s token from the vault. Returns `Ok(true)` if an
/// entry was deleted, `Ok(false)` if there was nothing to delete.
pub fn delete(provider: &str) -> Result<bool, ApiKeyError> {
    validate_provider(provider)?;
    let mut map = load_vault()?;
    if map.remove(provider).is_none() {
        return Ok(false);
    }
    save_vault(&map)?;
    Ok(true)
}

// ---------- Boot-time legacy migration ----------

/// Pure migration core: pull every legacy per-provider value the reader
/// still has, fold it into `map` (without clobbering an existing vault
/// entry — the vault is the canonical source after migration), and ask
/// the deleter to drop the legacy original. Returns the number of
/// legacy entries observed. Failures from the deleter only get logged
/// — the migration is idempotent across boots, so a transient deletion
/// failure just defers cleanup to the next boot.
fn fold_legacy_into_vault<R, D>(
    map: &mut HashMap<String, String>,
    providers: &[&str],
    mut read_legacy: R,
    mut delete_legacy: D,
) -> usize
where
    R: FnMut(&str) -> Result<Option<String>, ApiKeyError>,
    D: FnMut(&str) -> Result<(), ApiKeyError>,
{
    let mut migrated = 0usize;
    for provider in providers {
        match read_legacy(provider) {
            Ok(Some(token)) => {
                map.entry((*provider).to_string()).or_insert(token);
                if let Err(e) = delete_legacy(provider) {
                    tracing::warn!(
                        provider = provider,
                        error = %e,
                        "failed to delete legacy keychain entry; will retry on next boot",
                    );
                }
                migrated += 1;
            }
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(
                    provider = provider,
                    error = %e,
                    "failed to read legacy keychain entry; skipping",
                );
                continue;
            }
        }
    }
    migrated
}

/// Boot-time migration: fold any per-provider keychain entries from
/// before the single-blob vault into the vault, then delete the
/// originals. Idempotent — once the per-provider entries are gone the
/// next boot finds nothing to fold and is a no-op.
///
/// Failures during the fold are non-fatal and logged; the next boot
/// retries any legacy entries that survived. Only the final vault save
/// can return Err — and even that is a Keychain access failure, which
/// is handled at the call site by logging and continuing.
pub fn migrate_legacy_entries() -> Result<usize, ApiKeyError> {
    let mut map = load_vault()?;
    let migrated = fold_legacy_into_vault(
        &mut map,
        LEGACY_PROVIDERS,
        |p| read_keychain(p),
        |p| delete_keychain(p),
    );
    if migrated > 0 {
        save_vault(&map)?;
    }
    Ok(migrated)
}

// ---------- Tauri commands ----------

#[tauri::command]
pub async fn api_key_set(provider: String, token: String) -> Result<(), String> {
    set(&provider, &token).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_key_has(provider: String) -> Result<bool, String> {
    Ok(has(&provider))
}

#[tauri::command]
pub async fn api_key_delete(provider: String) -> Result<bool, String> {
    delete(&provider).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::Mutex as StdMutex;

    /// `reset_cache` mutates a process-global `OnceLock`; tests that
    /// touch the cache must serialize through this lock so two tests
    /// don't observe each other's intermediate state.
    static CACHE_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn validate_accepts_expected_providers() {
        for p in [
            "cloudflare",
            "hetzner",
            "digitalocean",
            "aws-s3",
            "do_2",
            // Agent runtime slots — same vault as infra providers; same
            // validator path so the SettingsModal "Agents" section
            // shares the surface.
            "anthropic",
            "anthropic-oauth",
            "openai",
            "ollama",
            // Identity passphrases (yah-identities.md P5) reuse the
            // vault under `identity:<fingerprint>` keys — make sure the
            // shape passes validation. Colons aren't allowed in our
            // provider names today; the Lock work will either widen the
            // validator or use a `_`-separated key like `identity_<fp>`.
        ] {
            assert!(validate_provider(p).is_ok(), "{p} should be valid");
        }
    }

    #[test]
    fn validate_rejects_garbage() {
        for p in ["", "with space", "slash/", "../escape", "semi;colon"] {
            assert!(validate_provider(p).is_err(), "{p:?} should be invalid");
        }
    }

    #[test]
    fn parse_vault_round_trips_through_serde() {
        let mut map = HashMap::new();
        map.insert("hetzner".to_string(), "tok-h".to_string());
        map.insert("openai".to_string(), "sk-o".to_string());
        let json = serde_json::to_string(&map).unwrap();
        let back = parse_vault(&json);
        assert_eq!(back.len(), 2);
        assert_eq!(back.get("hetzner"), Some(&"tok-h".to_string()));
        assert_eq!(back.get("openai"), Some(&"sk-o".to_string()));
    }

    #[test]
    fn parse_vault_empty_json_object() {
        let map = parse_vault("{}");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_vault_malformed_falls_back_to_empty() {
        // Hand-edited keychain entry shouldn't brick the app.
        let map = parse_vault("definitely not json");
        assert!(map.is_empty());
        let map2 = parse_vault("[\"array\", \"not\", \"object\"]");
        assert!(map2.is_empty());
    }

    #[test]
    fn fold_legacy_inserts_new_entries_and_deletes_originals() {
        let mut vault = HashMap::new();
        let legacy: HashMap<&str, String> = [
            ("hetzner", "tok-h".to_string()),
            ("openai", "sk-o".to_string()),
        ]
        .into_iter()
        .collect();
        let read_log: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let delete_log: RefCell<Vec<String>> = RefCell::new(Vec::new());

        let migrated = fold_legacy_into_vault(
            &mut vault,
            &["hetzner", "openai", "cloudflare"],
            |p| {
                read_log.borrow_mut().push(p.to_string());
                Ok(legacy.get(p).cloned())
            },
            |p| {
                delete_log.borrow_mut().push(p.to_string());
                Ok(())
            },
        );

        assert_eq!(migrated, 2);
        assert_eq!(vault.get("hetzner"), Some(&"tok-h".to_string()));
        assert_eq!(vault.get("openai"), Some(&"sk-o".to_string()));
        assert!(!vault.contains_key("cloudflare"), "legacy didn't have cloudflare");
        // Deleter only runs for entries that existed (read returned Some).
        let deletes = delete_log.into_inner();
        assert_eq!(deletes.len(), 2);
        assert!(deletes.contains(&"hetzner".to_string()));
        assert!(deletes.contains(&"openai".to_string()));
    }

    #[test]
    fn fold_legacy_does_not_clobber_existing_vault_entry() {
        // The vault is the canonical source after migration: if an
        // older yah wrote `hetzner=NEW` into the vault and the legacy
        // per-provider entry still says `OLD`, the new value wins.
        let mut vault = HashMap::new();
        vault.insert("hetzner".to_string(), "NEW".to_string());

        let migrated = fold_legacy_into_vault(
            &mut vault,
            &["hetzner"],
            |_| Ok(Some("OLD".to_string())),
            |_| Ok(()),
        );
        assert_eq!(migrated, 1);
        assert_eq!(vault.get("hetzner"), Some(&"NEW".to_string()));
    }

    #[test]
    fn fold_legacy_idempotent_when_nothing_to_migrate() {
        // Second-boot shape: no legacy entries left, vault stays put.
        let mut vault = HashMap::new();
        vault.insert("hetzner".to_string(), "tok".to_string());
        let delete_log: RefCell<usize> = RefCell::new(0);

        let migrated = fold_legacy_into_vault(
            &mut vault,
            &["hetzner", "openai", "cloudflare"],
            |_| Ok(None),
            |_| {
                *delete_log.borrow_mut() += 1;
                Ok(())
            },
        );
        assert_eq!(migrated, 0);
        // Vault untouched.
        assert_eq!(vault.get("hetzner"), Some(&"tok".to_string()));
        // Nothing to delete since nothing existed.
        assert_eq!(delete_log.into_inner(), 0);
    }

    #[test]
    fn fold_legacy_swallows_per_provider_read_errors() {
        // A keychain read failure on one provider must not block the
        // rest — solo-dev users should never end up with a partly-
        // migrated vault because, say, an ACL prompt got dismissed for
        // one item. The bad provider just gets retried next boot.
        let mut vault = HashMap::new();
        let migrated = fold_legacy_into_vault(
            &mut vault,
            &["hetzner", "openai"],
            |p| {
                if p == "hetzner" {
                    Err(ApiKeyError::Keychain(keyring::Error::Invalid(
                        "test".into(),
                        "denied".into(),
                    )))
                } else {
                    Ok(Some("sk-o".to_string()))
                }
            },
            |_| Ok(()),
        );
        assert_eq!(migrated, 1);
        assert_eq!(vault.get("openai"), Some(&"sk-o".to_string()));
        assert!(!vault.contains_key("hetzner"));
    }

    #[test]
    fn legacy_provider_list_covers_actual_constants_in_use() {
        // Any provider name a sibling module hands to api_keys::get
        // ought to also appear in LEGACY_PROVIDERS, otherwise an
        // existing per-provider keychain entry survives the migration
        // and the user pays an extra prompt forever. New providers must
        // be added here when introduced.
        for p in [
            "hetzner",
            "github",
            "openai",
            "anthropic",
            "anthropic-oauth",
            "ollama",
        ] {
            assert!(
                LEGACY_PROVIDERS.contains(&p),
                "provider {p:?} consumed by sibling modules but missing from LEGACY_PROVIDERS",
            );
        }
    }

    #[test]
    fn cache_reset_clears_loaded_state() {
        // Smoke test for the test-only reset hook itself; subsequent
        // tests rely on it to break inter-test contamination via the
        // process-global OnceLock.
        let _g = CACHE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        *cache().write().unwrap() = Some(HashMap::new());
        assert!(cache().read().unwrap().is_some());
        reset_cache();
        assert!(cache().read().unwrap().is_none());
    }
}

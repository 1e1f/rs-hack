//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Identity registry — SSH keys as first-class objects + cross-target
//! authorization records. P1 scope: data model, on-disk round-trip,
//! and the four Tauri commands the renderer drives the Settings →
//! Identities view from. **Probes don't live here yet** (P2/T2 adds
//! local-files / Hetzner / GitHub probe surfaces); every record's
//! `authorized_at` starts empty and stays empty until those land.
//!
//! See `.yah/arch/authored/yah-identities.md` for the full design including
//! probe semantics, cross-target ranking, and how the rig card consumes
//! the registry once P4 lands.
//!
//! Storage: `~/.yah/identities.json` (override parent via `YAH_HOME`),
//! camelCase serialized to match `rigs.json`'s precedent. yah-generated
//! private keys live in `~/.yah/keys/<name>` (mode `0600` on Unix);
//! imported keys are referenced by their existing path — yah never
//! copies private bytes.

use serde::{Deserialize, Serialize};
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Stable identifier for a keypair: SHA256 fingerprint of the public
/// key (the same form `ssh-keygen -lf` reports). Two records with the
/// same fingerprint are the same identity, regardless of file path,
/// algorithm tag, or comment.
pub type IdentityId = String; // "SHA256:abc…"

/// Single SSH keypair record. `authorized_at` is updated by probes and
/// authorize-writes (P2/P3); for T1 it always starts empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub id: IdentityId,
    pub name: String,
    pub algorithm: String,
    pub public_key: String,
    pub source: IdentitySource,
    #[serde(default)]
    pub authorized_at: Vec<Authorization>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
}

/// Where the private half of the keypair lives. Both branches keep
/// the bytes off the wire — this enum only carries paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IdentitySource {
    /// yah owns the keypair lifecycle: file lives under `~/.yah/keys/`
    /// and `identity_remove` deletes it on revocation.
    YahGenerated { private_key_path: PathBuf },
    /// yah only references this key — it lives wherever the user put
    /// it (typically `~/.ssh/`). `identity_remove` only drops the
    /// registry entry; the file stays.
    Imported {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_key_path: Option<PathBuf>,
        public_key_path: PathBuf,
    },
}

/// One "this identity is registered at <target>" record. Records are
/// caches with `last_seen` timestamps, not assertions of fact — probes
/// (P2) reconcile against ground truth and update / GC entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Authorization {
    Hetzner {
        project_id: String,
        key_id_in_hetzner: i64,
        name: String,
        last_seen: i64,
    },
    Github {
        account: String,
        key_id: i64,
        title: String,
        last_seen: i64,
    },
    Gitlab {
        instance: String,
        account: String,
        key_id: i64,
        title: String,
        last_seen: i64,
    },
    SshHost {
        user_at_host: String,
        last_seen: i64,
    },
}

/// On-disk schema for `~/.yah/identities.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentitiesFile {
    #[serde(default)]
    pub identities: Vec<Identity>,
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("invalid identity name {0:?}: must be ASCII alphanumeric + -/_, no path separators")]
    InvalidName(String),
    #[error("home directory not resolvable; set $HOME or $YAH_HOME")]
    NoHomeDir,
    #[error("an identity named {0:?} already exists at {1}")]
    AlreadyExists(String, String),
    #[error("public key file not found: {0}")]
    PublicKeyNotFound(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ssh-key error: {0}")]
    Key(#[from] ssh_key::Error),
    #[error("registry parse error: {0}")]
    Parse(serde_json::Error),
    #[error("registry write error: {0}")]
    Serialize(serde_json::Error),
}

fn validate_name(name: &str) -> Result<(), IdentityError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(IdentityError::InvalidName(name.to_string()))
    }
}

/// Resolve the yah home dir (`~/.yah` by default, `$YAH_HOME` override).
/// Used as the parent for both the registry file and the keys dir.
fn yah_home() -> Result<PathBuf, IdentityError> {
    if let Ok(p) = std::env::var("YAH_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(IdentityError::NoHomeDir)?;
    Ok(home.join(".yah"))
}

pub fn identities_file_path() -> Result<PathBuf, IdentityError> {
    Ok(yah_home()?.join("identities.json"))
}

/// Where yah-generated private keys live. Created lazily on first
/// `identity_create`; `0700` on Unix.
pub fn keys_dir() -> Result<PathBuf, IdentityError> {
    Ok(yah_home()?.join("keys"))
}

/// Read the registry. Missing or malformed file → empty registry; a
/// corrupt file shouldn't deadlock the app. Mirror of
/// `state::load_rigs_file` semantics.
pub fn load_file() -> IdentitiesFile {
    let Ok(p) = identities_file_path() else {
        return IdentitiesFile::default();
    };
    let Ok(bytes) = fs::read(&p) else {
        return IdentitiesFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|err| {
        tracing::warn!(error = %err, path = %p.display(), "identities.json malformed; ignoring");
        IdentitiesFile::default()
    })
}

pub fn save_file(file: &IdentitiesFile) -> Result<(), IdentityError> {
    let p = identities_file_path()?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
        set_dir_perms(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(IdentityError::Serialize)?;
    fs::write(&p, bytes)?;
    Ok(())
}

#[cfg(unix)]
fn set_dir_perms(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_dir_perms(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_perms(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_perms(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Generate a fresh ed25519 yah-managed identity. Writes the keypair
/// under `~/.yah/keys/<name>` and persists the registry entry. Refuses
/// to clobber an existing keyfile — renaming is the operator's call.
pub fn create(name: &str) -> Result<Identity, IdentityError> {
    validate_name(name)?;
    let dir = keys_dir()?;
    fs::create_dir_all(&dir)?;
    set_dir_perms(&dir)?;

    let private_path = dir.join(name);
    let public_path = dir.join(format!("{name}.pub"));
    if private_path.exists() || public_path.exists() {
        return Err(IdentityError::AlreadyExists(
            name.to_string(),
            private_path.to_string_lossy().into_owned(),
        ));
    }

    let mut private_key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)?;
    private_key.set_comment(format!("{name}@yah"));
    let openssh_private = private_key.to_openssh(LineEnding::LF)?;
    let openssh_public = private_key.public_key().to_openssh()?;

    fs::write(&private_path, openssh_private.as_bytes())?;
    fs::write(&public_path, format!("{openssh_public}\n"))?;
    set_private_perms(&private_path)?;

    let fingerprint = private_key.fingerprint(HashAlg::Sha256).to_string();
    let identity = Identity {
        id: fingerprint,
        name: name.to_string(),
        algorithm: private_key.algorithm().as_str().to_string(),
        public_key: openssh_public,
        source: IdentitySource::YahGenerated {
            private_key_path: private_path,
        },
        authorized_at: vec![],
        created_at: now_ms(),
        last_used_at: None,
    };

    let mut file = load_file();
    // De-dup: same fingerprint = same identity. Refresh display name
    // and source path, return the canonical record.
    if let Some(existing) = file.identities.iter_mut().find(|i| i.id == identity.id) {
        existing.name = identity.name.clone();
        existing.source = identity.source.clone();
        let canonical = existing.clone();
        save_file(&file)?;
        return Ok(canonical);
    }
    file.identities.push(identity.clone());
    save_file(&file)?;
    Ok(identity)
}

/// Reference an existing public key file (typically under `~/.ssh/`).
/// The private half — if present alongside — is recorded as a path
/// only; we never read or copy the bytes. `name` defaults to the
/// public-key filename's stem.
pub fn import(public_key_path: &Path, name: Option<&str>) -> Result<Identity, IdentityError> {
    if !public_key_path.is_file() {
        return Err(IdentityError::PublicKeyNotFound(
            public_key_path.to_string_lossy().into_owned(),
        ));
    }
    let text = fs::read_to_string(public_key_path)?;
    let trimmed = text.trim();
    let public = PublicKey::from_openssh(trimmed)?;
    let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
    let algorithm = public.algorithm().as_str().to_string();

    let derived_name = public_key_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-key")
        .to_string();
    let display_name = name.map(str::to_string).unwrap_or(derived_name);
    validate_name(&display_name)?;

    // Sibling private key, if any. Strip the `.pub` extension; if the
    // resulting path exists, record it. Never read its bytes.
    let private_candidate = public_key_path.with_extension("");
    let private_key_path = if private_candidate.is_file() {
        Some(private_candidate)
    } else {
        None
    };

    let identity = Identity {
        id: fingerprint,
        name: display_name,
        algorithm,
        public_key: trimmed.to_string(),
        source: IdentitySource::Imported {
            private_key_path,
            public_key_path: public_key_path.to_path_buf(),
        },
        authorized_at: vec![],
        created_at: now_ms(),
        last_used_at: None,
    };

    let mut file = load_file();
    if let Some(existing) = file.identities.iter_mut().find(|i| i.id == identity.id) {
        // Same fingerprint: refresh path/name in case the user is
        // re-importing after a move. Preserve `authorized_at` +
        // timestamps — those are state, the import is just metadata.
        existing.name = identity.name.clone();
        existing.source = identity.source.clone();
        let canonical = existing.clone();
        save_file(&file)?;
        return Ok(canonical);
    }
    file.identities.push(identity.clone());
    save_file(&file)?;
    Ok(identity)
}

/// Snapshot of every registered identity.
pub fn list() -> Vec<Identity> {
    load_file().identities
}

/// Drop one identity from the registry by id. For `YahGenerated`,
/// also deletes the on-disk keyfile pair best-effort (warns but
/// doesn't fail the call if the file is already gone). For `Imported`,
/// the user-owned files are never touched.
///
/// Returns whether the id was found.
pub fn remove(id: &str) -> Result<bool, IdentityError> {
    let mut file = load_file();
    let Some(idx) = file.identities.iter().position(|i| i.id == id) else {
        return Ok(false);
    };
    let removed = file.identities.remove(idx);
    if let IdentitySource::YahGenerated { private_key_path } = &removed.source {
        if let Err(e) = fs::remove_file(private_key_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %private_key_path.display(), "failed to delete yah-managed private key");
            }
        }
        let pub_path = private_key_path.with_extension("pub");
        if let Err(e) = fs::remove_file(&pub_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %pub_path.display(), "failed to delete yah-managed public key");
            }
        }
    }
    save_file(&file)?;
    Ok(true)
}

// ---------- Probes (P2) ----------
//
// Three discovery surfaces feed the registry: local key files, Hetzner
// project SSH keys, and GitHub account SSH keys. Probes are idempotent
// and best-effort — a missing PAT or transport error never aborts the
// other probes, and per-provider outcomes ride out via [`ProbeReport`]
// so the renderer can surface "couldn't check" rows separately from
// confirmed authorized / not-authorized rows.

const GITHUB_PROVIDER: &str = "github";
const GITHUB_API_BASE: &str = "https://api.github.com";
const HETZNER_DEFAULT_PROJECT_ID: &str = "default";

/// Outcome for one provider in a [`ProbeReport`]. `Ok.matches` counts
/// identities that got a (new or refreshed) Authorization record.
/// `Skipped` is the no-PAT / unconfigured path — surfaced distinctly so
/// the UI shows "configure GitHub token" rather than a red dot. `Error`
/// is a real upstream failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProbeOutcome {
    Ok { matches: usize },
    Skipped { reason: String },
    Error { reason: String },
}

/// Result of a fan-out probe pass. Local discovery is folded in as
/// `local_added`; per-provider outcomes carry their own state so the
/// renderer can render each row independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReport {
    pub identities_total: usize,
    pub local_added: usize,
    pub hetzner: ProbeOutcome,
    pub github: ProbeOutcome,
}

/// Per-identity probe outcome for the "re-check this row" action on a
/// rig card / settings row. `Found` carries the fresh Authorization;
/// `NotFound` is a confirmed negative (registry's stale record for this
/// provider gets cleared); `Skipped` / `Error` leave the registry alone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SingleProbeResult {
    Found { authorization: Authorization },
    NotFound,
    Skipped { reason: String },
    Error { reason: String },
}

#[derive(Debug, thiserror::Error)]
enum GithubProbeError {
    #[error("no GitHub token stored — set one in Settings → API Keys")]
    NoToken,
    #[error("keychain read failed: {0}")]
    Keychain(crate::api_keys::ApiKeyError),
    #[error("HTTP transport error: {0}")]
    Http(reqwest::Error),
    #[error("GitHub API returned {status}: {body}")]
    Upstream { status: u16, body: String },
}

/// Walk `$YAH_HOME/keys/*.pub` and `~/.ssh/*.pub`, fingerprinting each
/// parseable key. Identities not already in the registry are added —
/// keys under `$YAH_HOME/keys/` with a sibling private file land as
/// `YahGenerated` (rebuilds the record after a wiped identities.json
/// without losing the lifecycle marker); everything else lands as
/// `Imported`. Returns the count of newly-added records.
///
/// Discovery only — already-registered identities keep their
/// `authorized_at`; per-provider probes (below) own that.
pub fn probe_local_files() -> Result<usize, IdentityError> {
    let mut file = load_file();
    let mut known: HashSet<String> = file.identities.iter().map(|i| i.id.clone()).collect();
    let yah_keys = keys_dir().ok();
    let ssh_dir = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".ssh"));

    let mut search: Vec<&Path> = Vec::new();
    if let Some(p) = yah_keys.as_deref() {
        search.push(p);
    }
    if let Some(p) = ssh_dir.as_deref() {
        search.push(p);
    }

    let mut added = 0usize;
    for dir in search {
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("pub") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let trimmed = text.trim();
            let Ok(public) = PublicKey::from_openssh(trimmed) else {
                continue;
            };
            let id = public.fingerprint(HashAlg::Sha256).to_string();
            if known.contains(&id) {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("imported-key")
                .to_string();
            let private_candidate = path.with_extension("");
            let private_present = private_candidate.is_file();
            let in_yah_keys = yah_keys
                .as_ref()
                .map(|d| path.starts_with(d))
                .unwrap_or(false);
            let source = if in_yah_keys && private_present {
                IdentitySource::YahGenerated {
                    private_key_path: private_candidate,
                }
            } else {
                IdentitySource::Imported {
                    private_key_path: if private_present {
                        Some(private_candidate)
                    } else {
                        None
                    },
                    public_key_path: path.clone(),
                }
            };
            file.identities.push(Identity {
                id: id.clone(),
                name: stem,
                algorithm: public.algorithm().as_str().to_string(),
                public_key: trimmed.to_string(),
                source,
                authorized_at: vec![],
                created_at: now_ms(),
                last_used_at: None,
            });
            known.insert(id);
            added += 1;
        }
    }
    if added > 0 {
        save_file(&file)?;
    }
    Ok(added)
}

/// Re-fingerprint an OpenSSH public-key line (whatever the provider
/// returned) into our SHA256 registry id. Returns None for unparseable
/// lines — providers occasionally hand back malformed entries; dropping
/// them silently is the right default.
fn fingerprint_openssh(line: &str) -> Option<String> {
    PublicKey::from_openssh(line.trim())
        .ok()
        .map(|k| k.fingerprint(HashAlg::Sha256).to_string())
}

/// Pull every Hetzner-project SSH key, re-fingerprint each public-key
/// line with SHA256 (Hetzner serves MD5 fingerprints, no good for our
/// id format), and pair (id, Authorization::Hetzner) for the caller.
async fn fetch_hetzner_authorizations(
) -> Result<Vec<(String, Authorization)>, crate::hetzner::HetznerError> {
    let keys = crate::hetzner::list_ssh_keys().await?;
    let now = now_ms();
    let mut out = Vec::new();
    for k in keys {
        if let Some(id) = fingerprint_openssh(&k.public_key) {
            out.push((
                id,
                Authorization::Hetzner {
                    project_id: HETZNER_DEFAULT_PROJECT_ID.to_string(),
                    key_id_in_hetzner: k.id as i64,
                    name: k.name,
                    last_seen: now,
                },
            ));
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct GithubUserDto {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GithubKeyDto {
    id: i64,
    key: String,
    #[serde(default)]
    title: Option<String>,
}

/// `GET /user` + `GET /user/keys` against the GitHub REST API. Uses the
/// PAT stored under `api_keys::get("github")` — missing token resolves
/// to `NoToken` so the caller can surface a `Skipped` outcome rather
/// than a hard error.
async fn fetch_github_authorizations() -> Result<Vec<(String, Authorization)>, GithubProbeError> {
    let token = crate::api_keys::get(GITHUB_PROVIDER)
        .map_err(GithubProbeError::Keychain)?
        .ok_or(GithubProbeError::NoToken)?;
    let client = reqwest::Client::builder()
        .user_agent("yah-identities")
        .build()
        .map_err(GithubProbeError::Http)?;

    let user: GithubUserDto = github_get(&client, &token, "/user").await?;
    let keys: Vec<GithubKeyDto> = github_get(&client, &token, "/user/keys").await?;

    let now = now_ms();
    let mut out = Vec::new();
    for k in keys {
        if let Some(id) = fingerprint_openssh(&k.key) {
            out.push((
                id,
                Authorization::Github {
                    account: user.login.clone(),
                    key_id: k.id,
                    title: k.title.unwrap_or_default(),
                    last_seen: now,
                },
            ));
        }
    }
    Ok(out)
}

async fn github_get<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    path: &str,
) -> Result<T, GithubProbeError> {
    let resp = client
        .get(format!("{GITHUB_API_BASE}{path}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(GithubProbeError::Http)?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(GithubProbeError::Upstream {
            status: status.as_u16(),
            body,
        });
    }
    resp.json().await.map_err(GithubProbeError::Http)
}

fn hetzner_outcome(
    result: &Result<Vec<(String, Authorization)>, crate::hetzner::HetznerError>,
    identity_ids: &HashSet<String>,
) -> ProbeOutcome {
    match result {
        Ok(v) => {
            let matches = v.iter().filter(|(id, _)| identity_ids.contains(id)).count();
            ProbeOutcome::Ok { matches }
        }
        Err(crate::hetzner::HetznerError::NoToken) => ProbeOutcome::Skipped {
            reason: "no Hetzner token configured".into(),
        },
        Err(e) => ProbeOutcome::Error {
            reason: e.to_string(),
        },
    }
}

fn github_outcome(
    result: &Result<Vec<(String, Authorization)>, GithubProbeError>,
    identity_ids: &HashSet<String>,
) -> ProbeOutcome {
    match result {
        Ok(v) => {
            let matches = v.iter().filter(|(id, _)| identity_ids.contains(id)).count();
            ProbeOutcome::Ok { matches }
        }
        Err(GithubProbeError::NoToken) => ProbeOutcome::Skipped {
            reason: "no GitHub token configured".into(),
        },
        Err(e) => ProbeOutcome::Error {
            reason: e.to_string(),
        },
    }
}

/// Variant tag for replace-on-probe semantics. `probe_all` and the
/// per-identity probes both clear stale entries for a provider before
/// writing the fresh one — this is the predicate.
fn auth_kind(a: &Authorization) -> &'static str {
    match a {
        Authorization::Hetzner { .. } => "hetzner",
        Authorization::Github { .. } => "github",
        Authorization::Gitlab { .. } => "gitlab",
        Authorization::SshHost { .. } => "sshHost",
    }
}

/// Run every probe and reconcile the registry. Local discovery runs
/// first so newly-found keys can pick up provider auth in the same
/// pass. Each provider's outcome rides out independently so a Hetzner
/// 401 doesn't suppress a successful GitHub probe.
pub async fn probe_all() -> Result<ProbeReport, IdentityError> {
    let local_added = probe_local_files()?;
    let hetzner_result = fetch_hetzner_authorizations().await;
    let github_result = fetch_github_authorizations().await;

    let mut file = load_file();
    let identity_ids: HashSet<String> = file.identities.iter().map(|i| i.id.clone()).collect();
    let now = now_ms();

    let hetzner_map: std::collections::HashMap<String, Authorization> = match &hetzner_result {
        Ok(v) => v.iter().cloned().collect(),
        Err(_) => std::collections::HashMap::new(),
    };
    let github_map: std::collections::HashMap<String, Authorization> = match &github_result {
        Ok(v) => v.iter().cloned().collect(),
        Err(_) => std::collections::HashMap::new(),
    };

    for ident in &mut file.identities {
        if hetzner_result.is_ok() {
            ident.authorized_at.retain(|a| auth_kind(a) != "hetzner");
            if let Some(auth) = hetzner_map.get(&ident.id) {
                ident.authorized_at.push(auth.clone());
                ident.last_used_at = Some(now);
            }
        }
        if github_result.is_ok() {
            ident.authorized_at.retain(|a| auth_kind(a) != "github");
            if let Some(auth) = github_map.get(&ident.id) {
                ident.authorized_at.push(auth.clone());
                ident.last_used_at = Some(now);
            }
        }
    }
    save_file(&file)?;

    Ok(ProbeReport {
        identities_total: file.identities.len(),
        local_added,
        hetzner: hetzner_outcome(&hetzner_result, &identity_ids),
        github: github_outcome(&github_result, &identity_ids),
    })
}

/// Replace this identity's Authorization::Hetzner entry with whatever
/// Hetzner reports right now. Confirmed-not-registered drops the stale
/// entry; Skipped / Error leave the registry alone.
pub async fn probe_hetzner_one(id: &str) -> SingleProbeResult {
    let pulled = fetch_hetzner_authorizations().await;
    match pulled {
        Ok(v) => {
            let found = v.into_iter().find(|(rid, _)| rid == id).map(|(_, a)| a);
            let _ = update_single(id, |ident| {
                ident.authorized_at.retain(|a| auth_kind(a) != "hetzner");
                if let Some(auth) = &found {
                    ident.authorized_at.push(auth.clone());
                    ident.last_used_at = Some(now_ms());
                }
            });
            match found {
                Some(authorization) => SingleProbeResult::Found { authorization },
                None => SingleProbeResult::NotFound,
            }
        }
        Err(crate::hetzner::HetznerError::NoToken) => SingleProbeResult::Skipped {
            reason: "no Hetzner token configured".into(),
        },
        Err(e) => SingleProbeResult::Error {
            reason: e.to_string(),
        },
    }
}

/// GitHub-side mirror of [`probe_hetzner_one`].
pub async fn probe_github_one(id: &str) -> SingleProbeResult {
    let pulled = fetch_github_authorizations().await;
    match pulled {
        Ok(v) => {
            let found = v.into_iter().find(|(rid, _)| rid == id).map(|(_, a)| a);
            let _ = update_single(id, |ident| {
                ident.authorized_at.retain(|a| auth_kind(a) != "github");
                if let Some(auth) = &found {
                    ident.authorized_at.push(auth.clone());
                    ident.last_used_at = Some(now_ms());
                }
            });
            match found {
                Some(authorization) => SingleProbeResult::Found { authorization },
                None => SingleProbeResult::NotFound,
            }
        }
        Err(GithubProbeError::NoToken) => SingleProbeResult::Skipped {
            reason: "no GitHub token configured".into(),
        },
        Err(e) => SingleProbeResult::Error {
            reason: e.to_string(),
        },
    }
}

fn update_single<F: FnOnce(&mut Identity)>(id: &str, f: F) -> Result<bool, IdentityError> {
    let mut file = load_file();
    let Some(ident) = file.identities.iter_mut().find(|i| i.id == id) else {
        return Ok(false);
    };
    f(ident);
    save_file(&file)?;
    Ok(true)
}

// ---------- Authorize / deauthorize writes (P3) ----------
//
// Authorize is "register this key at this target." Deauthorize is the
// inverse. Both are thin shims over the provider client that update the
// registry's `authorized_at` cache after the provider call resolves —
// the registry stays the user-facing single view, so it has to mirror
// reality immediately rather than waiting for the next probe pass.

/// Register an identity's public key with the operator's Hetzner project.
/// Errors if the identity isn't in the registry. Updates the identity's
/// `authorized_at` with the fresh `Authorization::Hetzner` and bumps
/// `last_used_at`.
pub async fn authorize_hetzner(id: &str, name: &str) -> Result<Authorization, String> {
    let identity = lookup(id)?;
    let key = crate::hetzner::upload_ssh_key(name, &identity.public_key)
        .await
        .map_err(|e| e.to_string())?;
    let auth = Authorization::Hetzner {
        project_id: HETZNER_DEFAULT_PROJECT_ID.to_string(),
        key_id_in_hetzner: key.id as i64,
        name: key.name,
        last_seen: now_ms(),
    };
    write_authorization(id, &auth).map_err(|e| e.to_string())?;
    Ok(auth)
}

/// Deauthorize this identity at Hetzner: looks up the cached
/// `Authorization::Hetzner` to find the project-side key id, calls
/// DELETE, and clears the entry from the registry. Returns `false` if
/// the identity has no Hetzner authorization recorded — the renderer
/// can treat that as a no-op success.
pub async fn deauthorize_hetzner(id: &str) -> Result<bool, String> {
    let identity = lookup(id)?;
    let key_id = identity.authorized_at.iter().find_map(|a| match a {
        Authorization::Hetzner {
            key_id_in_hetzner, ..
        } => Some(*key_id_in_hetzner as u64),
        _ => None,
    });
    let Some(key_id) = key_id else {
        return Ok(false);
    };
    crate::hetzner::delete_ssh_key(key_id)
        .await
        .map_err(|e| e.to_string())?;
    update_single(id, |ident| {
        ident.authorized_at.retain(|a| auth_kind(a) != "hetzner");
    })
    .map_err(|e| e.to_string())?;
    Ok(true)
}

#[derive(Debug, Deserialize)]
struct GithubKeyCreatedDto {
    id: i64,
    #[serde(default)]
    title: Option<String>,
}

/// Register an identity's public key with the operator's GitHub
/// account. PAT must have `admin:public_key` scope; missing scope
/// surfaces as a 403 from the upstream call. Persists the resulting
/// `Authorization::Github` (with the github account login resolved via
/// `GET /user`).
pub async fn authorize_github(id: &str, title: &str) -> Result<Authorization, String> {
    let identity = lookup(id)?;
    let token = crate::api_keys::get(GITHUB_PROVIDER)
        .map_err(|e| e.to_string())?
        .ok_or("no GitHub token stored — set one in Settings → API Keys")?;
    let client = reqwest::Client::builder()
        .user_agent("yah-identities")
        .build()
        .map_err(|e| e.to_string())?;
    let user: GithubUserDto = github_get(&client, &token, "/user")
        .await
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({ "title": title, "key": identity.public_key });
    let resp = client
        .post(format!("{GITHUB_API_BASE}/user/keys"))
        .bearer_auth(&token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub returned {status}: {body}"));
    }
    let created: GithubKeyCreatedDto = resp.json().await.map_err(|e| e.to_string())?;

    let auth = Authorization::Github {
        account: user.login,
        key_id: created.id,
        title: created.title.unwrap_or_else(|| title.to_string()),
        last_seen: now_ms(),
    };
    write_authorization(id, &auth).map_err(|e| e.to_string())?;
    Ok(auth)
}

/// Deauthorize this identity at GitHub: DELETE /user/keys/<id> against
/// the cached key id and drop the entry from the registry. Returns
/// `false` when no Github authorization is recorded.
pub async fn deauthorize_github(id: &str) -> Result<bool, String> {
    let identity = lookup(id)?;
    let key_id = identity.authorized_at.iter().find_map(|a| match a {
        Authorization::Github { key_id, .. } => Some(*key_id),
        _ => None,
    });
    let Some(key_id) = key_id else {
        return Ok(false);
    };
    let token = crate::api_keys::get(GITHUB_PROVIDER)
        .map_err(|e| e.to_string())?
        .ok_or("no GitHub token stored — set one in Settings → API Keys")?;
    let client = reqwest::Client::builder()
        .user_agent("yah-identities")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .delete(format!("{GITHUB_API_BASE}/user/keys/{key_id}"))
        .bearer_auth(&token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    // 404 = key already gone; treat as idempotent success.
    if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub returned {status}: {body}"));
    }
    update_single(id, |ident| {
        ident.authorized_at.retain(|a| auth_kind(a) != "github");
    })
    .map_err(|e| e.to_string())?;
    Ok(true)
}

fn lookup(id: &str) -> Result<Identity, String> {
    load_file()
        .identities
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("identity {id} not in registry"))
}

/// Replace any existing Authorization of the same provider variant on
/// `identity[id]` with `auth`, then bump `last_used_at`. Used by both
/// authorize_* and the per-identity probe writes — keeps "one
/// authorization per provider per identity" invariant in one place.
fn write_authorization(id: &str, auth: &Authorization) -> Result<(), IdentityError> {
    let kind = auth_kind(auth);
    update_single(id, |ident| {
        ident.authorized_at.retain(|a| auth_kind(a) != kind);
        ident.authorized_at.push(auth.clone());
        ident.last_used_at = Some(now_ms());
    })?;
    Ok(())
}

// ---------- rigs.json keyPath → identityId migration (P5 / R034-T6) ----------
//
// Rigs predating the identity registry carried a `keyPath` (private-key
// path) per remote rig. This function fingerprints each `keyPath`'s
// public-key sibling, registers it as an `Imported` identity if absent,
// and writes the fingerprint back onto the rig as `identity_id`.
//
// Idempotent — already-migrated rigs (those with `identity_id` already
// set) are skipped on every boot. Safe to call unconditionally.

/// Walk `rigs.json`, populate the identity registry from each rig's
/// `key_path`, and stamp `identity_id` back onto the rig. Returns the
/// number of rigs whose `identity_id` was newly populated.
///
/// Failure modes are non-fatal: a missing `.pub` sibling, an
/// unparseable public key, or a rig pointing at a path that no longer
/// exists are all logged and skipped — boot continues with the rig
/// still using its `key_path` fallback. The only hard error is when
/// the registry file itself fails to write after a successful merge.
pub fn migrate_rigs_keypath_to_identity_id() -> Result<usize, IdentityError> {
    let mut rigs_file = crate::state::load_rigs_file();
    if rigs_file.rigs.is_empty() {
        return Ok(0);
    }
    let mut identities = load_file();
    let mut identities_dirty = false;
    let mut migrated = 0usize;
    let now = now_ms();

    for rig in &mut rigs_file.rigs {
        if rig.identity_id.is_some() {
            continue;
        }
        let Some(key_path) = rig.key_path.as_ref() else {
            continue;
        };
        let pub_path = derive_public_key_path(key_path);
        let Ok(text) = fs::read_to_string(&pub_path) else {
            tracing::warn!(
                rig = %rig.id.as_str(),
                path = %pub_path.display(),
                "rigs.json keyPath migration: public-key file not found; rig keeps keyPath fallback",
            );
            continue;
        };
        let trimmed = text.trim();
        let Ok(public) = PublicKey::from_openssh(trimmed) else {
            tracing::warn!(
                rig = %rig.id.as_str(),
                path = %pub_path.display(),
                "rigs.json keyPath migration: public-key file did not parse; rig keeps keyPath fallback",
            );
            continue;
        };
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();

        if !identities.identities.iter().any(|i| i.id == fingerprint) {
            // Sibling private key — record path only, never read bytes.
            // Imported semantics: yah doesn't own the lifecycle, so
            // identity_remove won't delete the user's file.
            let private_key_path = if key_path.is_file() {
                Some(key_path.clone())
            } else {
                None
            };
            let derived = pub_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("imported-key");
            let safe_name = sanitize_identity_name(derived);
            identities.identities.push(Identity {
                id: fingerprint.clone(),
                name: safe_name,
                algorithm: public.algorithm().as_str().to_string(),
                public_key: trimmed.to_string(),
                source: IdentitySource::Imported {
                    private_key_path,
                    public_key_path: pub_path,
                },
                authorized_at: vec![],
                created_at: now,
                last_used_at: None,
            });
            identities_dirty = true;
        }

        rig.identity_id = Some(fingerprint);
        migrated += 1;
    }

    if identities_dirty {
        save_file(&identities)?;
    }
    if migrated > 0 {
        crate::state::save_rigs_file(&rigs_file).map_err(IdentityError::Io)?;
    }
    Ok(migrated)
}

/// `~/.ssh/id_ed25519` → `~/.ssh/id_ed25519.pub`. If the path already
/// ends in `.pub`, return it unchanged so callers can pass either half
/// of the keypair.
fn derive_public_key_path(key_path: &Path) -> PathBuf {
    if key_path.extension().and_then(|s| s.to_str()) == Some("pub") {
        return key_path.to_path_buf();
    }
    let mut bytes = key_path.as_os_str().to_owned();
    bytes.push(".pub");
    PathBuf::from(bytes)
}

/// Coerce an arbitrary filesystem stem into something `validate_name`
/// accepts. Migration is best-effort: a bad name shouldn't block the
/// rig from being able to use the identity registry.
fn sanitize_identity_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        s.push_str("imported-key");
    }
    if s.len() > 64 {
        s.truncate(64);
    }
    s
}

// ---------- Tauri-managed serializing state ----------
//
// Every command that mutates the registry takes this lock before the
// load → mutate → save sequence so concurrent invokes don't race on
// the JSON file. Reads (list) take the lock too for ordering with
// in-flight writes.

/// Tauri-managed gate around the registry's read-modify-write
/// critical section. Held briefly per command; the underlying file
/// I/O is the slow part, not the lock.
#[derive(Clone, Default)]
pub struct IdentitiesState {
    inner: Arc<Mutex<()>>,
}

impl IdentitiesState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------- Tauri commands ----------

#[tauri::command]
pub async fn identity_list(
    state: tauri::State<'_, IdentitiesState>,
) -> Result<Vec<Identity>, String> {
    let _guard = state.inner.lock().await;
    Ok(list())
}

#[tauri::command]
pub async fn identity_create(
    state: tauri::State<'_, IdentitiesState>,
    name: String,
) -> Result<Identity, String> {
    let _guard = state.inner.lock().await;
    create(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn identity_import(
    state: tauri::State<'_, IdentitiesState>,
    public_key_path: String,
    name: Option<String>,
) -> Result<Identity, String> {
    let _guard = state.inner.lock().await;
    import(Path::new(&public_key_path), name.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn identity_remove(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
) -> Result<bool, String> {
    let _guard = state.inner.lock().await;
    remove(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn identity_probe_all(
    state: tauri::State<'_, IdentitiesState>,
) -> Result<ProbeReport, String> {
    let _guard = state.inner.lock().await;
    probe_all().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn identity_probe_hetzner(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
) -> Result<SingleProbeResult, String> {
    let _guard = state.inner.lock().await;
    Ok(probe_hetzner_one(&id).await)
}

#[tauri::command]
pub async fn identity_probe_github(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
) -> Result<SingleProbeResult, String> {
    let _guard = state.inner.lock().await;
    Ok(probe_github_one(&id).await)
}

#[tauri::command]
pub async fn identity_authorize_hetzner(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
    name: String,
) -> Result<Authorization, String> {
    let _guard = state.inner.lock().await;
    authorize_hetzner(&id, &name).await
}

#[tauri::command]
pub async fn identity_deauthorize_hetzner(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
) -> Result<bool, String> {
    let _guard = state.inner.lock().await;
    deauthorize_hetzner(&id).await
}

#[tauri::command]
pub async fn identity_authorize_github(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
    title: String,
) -> Result<Authorization, String> {
    let _guard = state.inner.lock().await;
    authorize_github(&id, &title).await
}

#[tauri::command]
pub async fn identity_deauthorize_github(
    state: tauri::State<'_, IdentitiesState>,
    id: String,
) -> Result<bool, String> {
    let _guard = state.inner.lock().await;
    deauthorize_github(&id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// Tests mutate `$YAH_HOME` so they run under per-test temp dirs;
    /// since std::env is process-global, serialize tests through this
    /// mutex so two tests don't race on the same env var.
    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn with_temp_yah_home<F: FnOnce(&Path) -> R, R>(f: F) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = std::env::temp_dir().join(format!(
            "yah-identities-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let prev = std::env::var_os("YAH_HOME");
        std::env::set_var("YAH_HOME", &tmp);
        let result = f(&tmp);
        match prev {
            Some(v) => std::env::set_var("YAH_HOME", v),
            None => std::env::remove_var("YAH_HOME"),
        }
        let _ = fs::remove_dir_all(&tmp);
        result
    }

    #[test]
    fn validate_accepts_reasonable_names() {
        for n in ["yah-personal", "id_ed25519", "test_1", "my-key-42"] {
            assert!(validate_name(n).is_ok(), "{n} should be valid");
        }
    }

    #[test]
    fn validate_rejects_paths_and_unicode() {
        for n in [
            "",
            "with space",
            "../escape",
            "slash/key",
            "with.dot",
            "ñame",
        ] {
            assert!(validate_name(n).is_err(), "{n:?} should be invalid");
        }
    }

    #[test]
    fn identities_file_round_trips_through_serde() {
        let f = IdentitiesFile {
            identities: vec![Identity {
                id: "SHA256:abc".into(),
                name: "yah-personal".into(),
                algorithm: "ssh-ed25519".into(),
                public_key: "ssh-ed25519 AAAA…".into(),
                source: IdentitySource::YahGenerated {
                    private_key_path: PathBuf::from("/Users/leif/.yah/keys/yah-personal"),
                },
                authorized_at: vec![Authorization::Hetzner {
                    project_id: "default".into(),
                    key_id_in_hetzner: 12345,
                    name: "yah-personal".into(),
                    last_seen: 1_745_875_200_000,
                }],
                created_at: 1_745_875_000_000,
                last_used_at: Some(1_745_880_000_000),
            }],
        };
        let json = serde_json::to_string(&f).unwrap();
        // camelCase + tagged-enum shape per the architecture doc.
        assert!(json.contains("authorizedAt"), "{json}");
        assert!(json.contains("createdAt"), "{json}");
        assert!(json.contains("lastUsedAt"), "{json}");
        assert!(json.contains("\"kind\":\"yahGenerated\""), "{json}");
        assert!(json.contains("\"kind\":\"hetzner\""), "{json}");
        let back: IdentitiesFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.identities.len(), 1);
        assert_eq!(back.identities[0].id, "SHA256:abc");
    }

    #[test]
    fn create_round_trips_through_list_and_remove_deletes_keyfile() {
        with_temp_yah_home(|home| {
            let id = create("yah_test").expect("create");
            assert_eq!(id.algorithm, "ssh-ed25519");
            assert!(id.public_key.starts_with("ssh-ed25519 "));
            assert!(id.id.starts_with("SHA256:"));
            assert!(matches!(id.source, IdentitySource::YahGenerated { .. }));

            // Files exist on disk under ~/.yah/keys/.
            let private = home.join("keys").join("yah_test");
            let public = home.join("keys").join("yah_test.pub");
            assert!(private.is_file(), "private key should exist at {private:?}");
            assert!(public.is_file(), "public key should exist at {public:?}");

            // Registry round-trips through identities.json.
            let listed = list();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, id.id);

            // Same name twice rejects clobber.
            assert!(matches!(
                create("yah_test"),
                Err(IdentityError::AlreadyExists(_, _))
            ));

            // Remove deletes the keyfiles.
            assert!(remove(&id.id).expect("remove"));
            assert!(!private.exists(), "private key should be deleted");
            assert!(!public.exists(), "public key should be deleted");
            assert!(list().is_empty(), "registry should be empty after remove");
        });
    }

    #[test]
    fn import_does_not_copy_private_key_bytes() {
        with_temp_yah_home(|home| {
            // Generate a key under ~/.ssh/ via raw ssh-key crate (not
            // `create`, which would put it under ~/.yah/keys/).
            let ssh_dir = home.join(".ssh-fixture");
            fs::create_dir_all(&ssh_dir).unwrap();
            let private_path = ssh_dir.join("id_test");
            let public_path = ssh_dir.join("id_test.pub");
            let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
            fs::write(
                &private_path,
                key.to_openssh(LineEnding::LF).unwrap().as_bytes(),
            )
            .unwrap();
            fs::write(
                &public_path,
                format!("{}\n", key.public_key().to_openssh().unwrap()),
            )
            .unwrap();

            let id = import(&public_path, None).expect("import");
            assert_eq!(id.id, key.fingerprint(HashAlg::Sha256).to_string());
            assert_eq!(id.name, "id_test"); // derived from filename
            match &id.source {
                IdentitySource::Imported {
                    private_key_path: Some(p),
                    public_key_path: pub_p,
                } => {
                    assert_eq!(p, &private_path);
                    assert_eq!(pub_p, &public_path);
                }
                other => panic!("expected Imported with private path, got {other:?}"),
            }

            // Original private key file untouched (yah did not copy).
            assert!(private_path.is_file());
            // Removing the import drops the registry entry but leaves
            // the user's files alone.
            assert!(remove(&id.id).expect("remove"));
            assert!(
                private_path.is_file(),
                "imported private key file must remain after remove"
            );
            assert!(
                public_path.is_file(),
                "imported public key file must remain after remove"
            );
        });
    }

    #[test]
    fn probe_local_picks_up_yah_keys_and_ssh_keys_then_no_ops() {
        with_temp_yah_home(|home| {
            // 1. Yah-managed key under $YAH_HOME/keys/ (private+public).
            let kdir = home.join("keys");
            fs::create_dir_all(&kdir).unwrap();
            let yah_priv = kdir.join("yah-managed");
            let yah_pub = kdir.join("yah-managed.pub");
            let yah_key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
            fs::write(
                &yah_priv,
                yah_key.to_openssh(LineEnding::LF).unwrap().as_bytes(),
            )
            .unwrap();
            fs::write(
                &yah_pub,
                format!("{}\n", yah_key.public_key().to_openssh().unwrap()),
            )
            .unwrap();

            // 2. User key under "$HOME/.ssh" — point HOME at our temp.
            let prev_home = std::env::var_os("HOME");
            std::env::set_var("HOME", home);
            let ssh_dir = home.join(".ssh");
            fs::create_dir_all(&ssh_dir).unwrap();
            let user_pub = ssh_dir.join("id_ed25519.pub");
            let user_key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
            fs::write(
                &user_pub,
                format!("{}\n", user_key.public_key().to_openssh().unwrap()),
            )
            .unwrap();

            // First probe pass adds both records.
            let added = probe_local_files().expect("probe_local_files");
            assert_eq!(added, 2, "should import both keys");
            let listed = list();
            assert_eq!(listed.len(), 2);
            let yah_id = yah_key.fingerprint(HashAlg::Sha256).to_string();
            let user_id = user_key.fingerprint(HashAlg::Sha256).to_string();
            let yah_entry = listed.iter().find(|i| i.id == yah_id).unwrap();
            assert!(matches!(
                yah_entry.source,
                IdentitySource::YahGenerated { .. }
            ));
            let user_entry = listed.iter().find(|i| i.id == user_id).unwrap();
            assert!(matches!(user_entry.source, IdentitySource::Imported { .. }));

            // Second pass is idempotent — known fingerprints skipped.
            let added2 = probe_local_files().expect("probe_local_files 2");
            assert_eq!(added2, 0);
            assert_eq!(list().len(), 2);

            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        });
    }

    #[test]
    fn fingerprint_openssh_matches_sha256_form() {
        let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
        let line = key.public_key().to_openssh().unwrap();
        let direct = key.fingerprint(HashAlg::Sha256).to_string();
        let via_helper = fingerprint_openssh(&line).expect("parses");
        assert_eq!(via_helper, direct);
        assert!(via_helper.starts_with("SHA256:"));
        // Garbage in -> None.
        assert!(fingerprint_openssh("not a key").is_none());
    }

    #[test]
    fn probe_report_serializes_with_camel_case_and_tagged_outcome() {
        let r = ProbeReport {
            identities_total: 2,
            local_added: 1,
            hetzner: ProbeOutcome::Ok { matches: 1 },
            github: ProbeOutcome::Skipped {
                reason: "no GitHub token configured".into(),
            },
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("identitiesTotal"), "{json}");
        assert!(json.contains("localAdded"), "{json}");
        assert!(json.contains("\"kind\":\"ok\""), "{json}");
        assert!(json.contains("\"kind\":\"skipped\""), "{json}");
        let back: ProbeReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.identities_total, 2);
        assert!(matches!(back.hetzner, ProbeOutcome::Ok { matches: 1 }));
    }

    #[test]
    fn deauthorize_returns_false_when_identity_has_no_authorization_for_provider() {
        with_temp_yah_home(|_home| {
            // Identity exists but its authorized_at is empty for both
            // providers — deauthorize is a no-op success, not an error.
            let id = create("yah_test_deauth").expect("create");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let hetz = rt.block_on(deauthorize_hetzner(&id.id)).expect("hetzner");
            assert!(!hetz);
            let gh = rt.block_on(deauthorize_github(&id.id)).expect("github");
            assert!(!gh);
        });
    }

    #[test]
    fn write_authorization_replaces_same_provider_and_preserves_others() {
        with_temp_yah_home(|_home| {
            let id = create("yah_test_writeauth").expect("create");
            let h1 = Authorization::Hetzner {
                project_id: "default".into(),
                key_id_in_hetzner: 111,
                name: "first".into(),
                last_seen: 1_000,
            };
            let h2 = Authorization::Hetzner {
                project_id: "default".into(),
                key_id_in_hetzner: 222,
                name: "second".into(),
                last_seen: 2_000,
            };
            let g = Authorization::Github {
                account: "leif".into(),
                key_id: 999,
                title: "yah".into(),
                last_seen: 3_000,
            };
            write_authorization(&id.id, &h1).unwrap();
            write_authorization(&id.id, &g).unwrap();
            write_authorization(&id.id, &h2).unwrap();
            let after = list().into_iter().find(|i| i.id == id.id).unwrap();
            assert_eq!(after.authorized_at.len(), 2);
            // Hetzner entry replaced (key_id_in_hetzner == 222), GitHub
            // entry preserved.
            assert!(after.authorized_at.iter().any(|a| matches!(
                a,
                Authorization::Hetzner {
                    key_id_in_hetzner: 222,
                    ..
                }
            )));
            assert!(after
                .authorized_at
                .iter()
                .any(|a| matches!(a, Authorization::Github { key_id: 999, .. })));
            assert!(after.last_used_at.is_some());
        });
    }

    #[test]
    fn lookup_errors_on_unknown_id() {
        with_temp_yah_home(|_home| {
            let err = lookup("SHA256:does-not-exist").expect_err("must fail");
            assert!(err.contains("not in registry"), "{err}");
        });
    }

    #[test]
    fn migrate_keypath_imports_identity_and_stamps_rig() {
        with_temp_yah_home(|home| {
            // Stage a private+public keypair the way a remote rig would
            // have it on disk before the identity registry existed.
            let ssh_dir = home.join("ssh-fixture");
            fs::create_dir_all(&ssh_dir).unwrap();
            let private_path = ssh_dir.join("id_remote");
            let public_path = ssh_dir.join("id_remote.pub");
            let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
            fs::write(
                &private_path,
                key.to_openssh(LineEnding::LF).unwrap().as_bytes(),
            )
            .unwrap();
            fs::write(
                &public_path,
                format!("{}\n", key.public_key().to_openssh().unwrap()),
            )
            .unwrap();
            let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();

            // Persist a rigs.json with one remote rig that only carries
            // keyPath — the legacy shape this migration targets.
            let rig = crate::state::Rig {
                id: crate::state::RigId("rig:legacy01".into()),
                name: "legacy".into(),
                path: PathBuf::from("/srv/code"),
                kind: crate::state::RigKind::Remote,
                last_active_at: None,
                host: Some("box.example.com".into()),
                port: None,
                user: Some("agent".into()),
                key_path: Some(private_path.clone()),
                identity_id: None,
            };
            let rigs_file = crate::state::RigsFile {
                rigs: vec![rig],
                last_active: None,
            };
            crate::state::save_rigs_file(&rigs_file).unwrap();

            // First migration pass: imports the identity, stamps the rig.
            let migrated = migrate_rigs_keypath_to_identity_id().expect("migrate");
            assert_eq!(migrated, 1);

            let listed = list();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, fingerprint);
            assert_eq!(listed[0].name, "id_remote");
            match &listed[0].source {
                IdentitySource::Imported {
                    private_key_path: Some(p),
                    public_key_path: pub_p,
                } => {
                    assert_eq!(p, &private_path);
                    assert_eq!(pub_p, &public_path);
                }
                other => panic!("expected Imported with private path, got {other:?}"),
            }
            // Original key files untouched.
            assert!(private_path.is_file());
            assert!(public_path.is_file());

            let reloaded = crate::state::load_rigs_file();
            assert_eq!(
                reloaded.rigs[0].identity_id.as_deref(),
                Some(fingerprint.as_str())
            );
            // keyPath kept as a fallback for one release.
            assert_eq!(reloaded.rigs[0].key_path.as_ref(), Some(&private_path));

            // Second pass is a no-op — already migrated.
            let again = migrate_rigs_keypath_to_identity_id().expect("migrate idempotent");
            assert_eq!(again, 0);
            assert_eq!(list().len(), 1);
        });
    }

    #[test]
    fn migrate_keypath_reuses_existing_identity_by_fingerprint() {
        with_temp_yah_home(|home| {
            let ssh_dir = home.join("ssh-fixture");
            fs::create_dir_all(&ssh_dir).unwrap();
            let private_path = ssh_dir.join("id_remote");
            let public_path = ssh_dir.join("id_remote.pub");
            let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
            fs::write(
                &private_path,
                key.to_openssh(LineEnding::LF).unwrap().as_bytes(),
            )
            .unwrap();
            fs::write(
                &public_path,
                format!("{}\n", key.public_key().to_openssh().unwrap()),
            )
            .unwrap();

            // Pre-existing identity in the registry — same fingerprint.
            let existing = import(&public_path, Some("preexisting")).expect("import");

            // Rig points at the same keyPath.
            let rigs_file = crate::state::RigsFile {
                rigs: vec![crate::state::Rig {
                    id: crate::state::RigId("rig:legacy02".into()),
                    name: "legacy".into(),
                    path: PathBuf::from("/srv/code"),
                    kind: crate::state::RigKind::Remote,
                    last_active_at: None,
                    host: Some("box.example.com".into()),
                    port: None,
                    user: Some("agent".into()),
                    key_path: Some(private_path.clone()),
                    identity_id: None,
                }],
                last_active: None,
            };
            crate::state::save_rigs_file(&rigs_file).unwrap();

            let migrated = migrate_rigs_keypath_to_identity_id().expect("migrate");
            assert_eq!(migrated, 1);

            // Registry still has exactly the one identity — fingerprint
            // dedup ran instead of producing a second record.
            let listed = list();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, existing.id);
            // Pre-existing display name preserved (migration didn't
            // clobber it).
            assert_eq!(listed[0].name, "preexisting");

            let reloaded = crate::state::load_rigs_file();
            assert_eq!(
                reloaded.rigs[0].identity_id.as_deref(),
                Some(existing.id.as_str())
            );
        });
    }

    #[test]
    fn migrate_keypath_skips_rig_when_pub_file_missing() {
        with_temp_yah_home(|home| {
            // keyPath that doesn't exist on disk at all — boot must not
            // fail, the rig just keeps using its keyPath fallback.
            let phantom = home.join("missing-key");
            let rigs_file = crate::state::RigsFile {
                rigs: vec![crate::state::Rig {
                    id: crate::state::RigId("rig:legacy03".into()),
                    name: "legacy".into(),
                    path: PathBuf::from("/srv/code"),
                    kind: crate::state::RigKind::Remote,
                    last_active_at: None,
                    host: Some("box.example.com".into()),
                    port: None,
                    user: Some("agent".into()),
                    key_path: Some(phantom),
                    identity_id: None,
                }],
                last_active: None,
            };
            crate::state::save_rigs_file(&rigs_file).unwrap();

            let migrated = migrate_rigs_keypath_to_identity_id().expect("migrate");
            assert_eq!(migrated, 0);
            assert!(list().is_empty());
            let reloaded = crate::state::load_rigs_file();
            assert!(reloaded.rigs[0].identity_id.is_none());
        });
    }

    #[test]
    fn duplicate_import_returns_canonical_record_with_refreshed_name() {
        with_temp_yah_home(|home| {
            let ssh_dir = home.join(".ssh-fixture");
            fs::create_dir_all(&ssh_dir).unwrap();
            let public_path = ssh_dir.join("id_test.pub");
            let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap();
            fs::write(
                &public_path,
                format!("{}\n", key.public_key().to_openssh().unwrap()),
            )
            .unwrap();

            let first = import(&public_path, Some("first-name")).expect("first");
            let second = import(&public_path, Some("renamed")).expect("second");
            assert_eq!(first.id, second.id);
            assert_eq!(second.name, "renamed");
            // Registry contains exactly one entry.
            assert_eq!(list().len(), 1);
        });
    }
}

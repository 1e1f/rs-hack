//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Local SSH key discovery + ed25519 keypair generation. Backs R029's
//! provision flow: the operator either picks an existing local key
//! (`~/.ssh/*.pub`) or asks yah to generate a fresh one, then the
//! public half gets uploaded to Hetzner via [`crate::hetzner`] for use
//! at server-creation time.
//!
//! Generation defaults to **ed25519** — modern SSH default, smaller
//! than RSA, supported by every Hetzner image. Private keys are
//! written with mode `0600` on Unix (Windows ACLs are inherited from
//! the user profile by default; an explicit ACL pass would be a
//! follow-up if Windows operator demand materialises).
//!
//! Tauri commands exposed to the renderer:
//!
//! * `ssh_key_list_local() -> Vec<LocalSshKey>`
//! * `ssh_key_generate(name) -> LocalSshKey`
//!
//! Private-key contents never reach the renderer. The DTO only carries
//! the public half + path metadata; the renderer never has cause to
//! read or display the private bytes.
//!
//! @yah:ticket(R034-T2, "Identity probes: local files + Hetzner + GitHub (best-effort, missing PAT skips silently)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R034)
//! @arch:see(architecture/yah-identities.md)
//! @yah:handoff("Identity probes (P2) landed end-to-end. New surfaces in app/tauri/src/identities.rs: probe_local_files() walks $YAH_HOME/keys + ~/.ssh/*.pub, fingerprints with SHA256, merges new identities into the registry — yah-keys-dir keys with sibling private files land as YahGenerated (recovers the lifecycle marker after a wiped identities.json), everything else lands as Imported. fetch_hetzner_authorizations() reuses crate::hetzner::list_ssh_keys and re-fingerprints each public_key line via fingerprint_openssh helper (Hetzner serves MD5 fingerprints, no good for our SHA256 id format). fetch_github_authorizations() calls GET /user + /user/keys via reqwest with the PAT from api_keys::get(\"github\") — User-Agent yah-identities, Accept application/vnd.github+json, X-GitHub-Api-Version 2022-11-28; missing token resolves to GithubProbeError::NoToken so the caller surfaces ProbeOutcome::Skipped rather than a hard error. probe_all() fans out: local first, then Hetzner + GitHub independently; reconciles by clearing stale Authorization::{Hetzner|Github} entries before writing fresh ones (only when that provider's probe succeeded — Skipped/Error leaves the registry alone), bumps last_used_at on any identity that picked up an Authorization. Returns ProbeReport { identities_total, local_added, hetzner: ProbeOutcome, github: ProbeOutcome }. Per-identity probe_hetzner_one(id)/probe_github_one(id) return SingleProbeResult { Found{authorization} | NotFound | Skipped{reason} | Error{reason} } and replace the matching Authorization variant on that one identity (NotFound clears the stale record). Three new Tauri commands wired in lib.rs invoke_handler: identity_probe_all, identity_probe_hetzner, identity_probe_github — each takes the IdentitiesState mutex so probes serialize with create/import/remove on the same identities.json. Tests: 9/9 in identities::tests including probe_local_picks_up_yah_keys_and_ssh_keys_then_no_ops (idempotent across two passes), fingerprint_openssh_matches_sha256_form, probe_report_serializes_with_camel_case_and_tagged_outcome (asserts identitiesTotal/localAdded camelCase + kind:\"ok\"/kind:\"skipped\" tagged enum). cargo build -p yah-tauri green; cargo test -p yah-tauri --lib 68/68 green; bun run typecheck clean; bun run build 1698 modules / 4.21MB.")
//! @yah:next("T3 (P3 writes): identity_authorize_hetzner(id, name) is a thin shim over hetzner::upload_ssh_key — call it then refresh the registry's Authorization::Hetzner via probe_hetzner_one to pick up the assigned key_id_in_hetzner. identity_authorize_github(id, title) is new: POST https://api.github.com/user/keys with {title, key} body, then probe to pick up the returned id. identity_deauthorize_* call DELETE /v1/ssh_keys/<id> (Hetzner) / DELETE /user/keys/<id> (GitHub) and clear the matching Authorization variant from the identity's authorized_at.")
//! @yah:next("F4/F5 (P4 renderer): now unblocked. env adapter additions: rpc.identity surface in yah-ui/src/env/index.ts (list/create/import/remove + probeAll/probeHetzner/probeGithub); types.ts gains Identity, IdentitySource, Authorization, ProbeReport, ProbeOutcome, SingleProbeResult mirrors of the Rust types; tauri.ts wires invoke('identity_*'); browser.ts returns mock data for component inspection. Settings → Identities section consumes rpc.identity; Rig card identity row consumes the same.")
//! @yah:next("F4 should call identity_probe_all() on Settings open and on Settings → Identities mount; F5 should call it on remote-rig card mount, scoped to the providers the rig cares about (Hetzner if Hetzner-provisioned, plus whichever forge the rig's git remote points at). last_used_at is updated for any identity that picks up an Authorization so the ranking-by-last-used pick in the picker stays fresh without a separate write path.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib identities")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//! @yah:gotcha("fetch_github_authorizations() needs a github PAT in api_keys under provider name 'github' — api_keys::validate_provider already accepts that string (ASCII alphanumeric + -/_). The original T1 next-step said to allowlist it; turns out validate_provider has no allowlist, just a charset check, so no api_keys.rs change was needed.")
//! @yah:gotcha("Hetzner project_id in Authorization::Hetzner is hardcoded to 'default' since each install authenticates against a single Hetzner project (see settings-api-keys.md threat-model). When multi-project lands the call site needs to plumb the project label through fetch_hetzner_authorizations.")
//! @yah:assumes("GitHub /user/keys's 'key' field is the full OpenSSH single-line public key (algo + base64 + optional comment). Confirmed against the API docs but not exercised against a live PAT in tests — first real probe pass under a configured token is the verify.")

use serde::Serialize;
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use std::fs;
use std::path::{Path, PathBuf};

/// Renderer-facing summary of a key found in (or written to) `~/.ssh/`.
/// `public_key` is the single-line OpenSSH public-key string the
/// renderer can display + ship straight to `hetzner_upload_ssh_key`.
#[derive(Debug, Clone, Serialize)]
pub struct LocalSshKey {
    /// Filename stem (e.g. `id_ed25519`). The picker uses this as the
    /// label.
    pub name: String,
    /// Absolute path to the `.pub` file.
    pub public_key_path: String,
    /// `ssh-ed25519 AAAA… [comment]` — the line as written on disk.
    pub public_key: String,
    /// `SHA256:…` — same fingerprint format `ssh -o "FingerprintHash=sha256"`
    /// reports, so operators can eyeball the match.
    pub fingerprint: String,
    /// Algorithm tag (`ssh-ed25519`, `ssh-rsa`, etc.) lifted from the
    /// public-key line. Keys with unparseable algorithms are dropped at
    /// listing time, so this is always a real, supported value.
    pub algorithm: String,
    /// Whether the matching private key (filename without `.pub`) is
    /// present alongside. Renderer surfaces a "public-only" hint when
    /// false — the user can still upload the public half but won't be
    /// able to ssh from this machine without recovering the private.
    pub has_private: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SshKeyError {
    #[error("invalid key name {0:?}: must be ASCII alphanumeric + -/_, no path separators")]
    InvalidName(String),
    #[error("home directory not resolvable; set $HOME")]
    NoHomeDir,
    #[error("a key named {0:?} already exists at {1}")]
    AlreadyExists(String, String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ssh-key error: {0}")]
    Key(#[from] ssh_key::Error),
}

fn validate_key_name(name: &str) -> Result<(), SshKeyError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(SshKeyError::InvalidName(name.to_string()))
    }
}

fn ssh_dir() -> Result<PathBuf, SshKeyError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(SshKeyError::NoHomeDir)?;
    Ok(home.join(".ssh"))
}

/// Enumerate every parseable `*.pub` file under `~/.ssh/`. Unparseable
/// public-key files (corrupt, wrong format, stray .pub-suffixed files)
/// are silently dropped — the picker only shows keys we can actually
/// upload. Sorted by filename for stable rendering.
pub fn list_local() -> Result<Vec<LocalSshKey>, SshKeyError> {
    let dir = match ssh_dir() {
        Ok(d) => d,
        Err(SshKeyError::NoHomeDir) => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("pub") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string)
        else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let trimmed = text.trim();
        let Ok(public) = ssh_key::PublicKey::from_openssh(trimmed) else {
            continue;
        };
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        let algorithm = public.algorithm().as_str().to_string();
        let private_path = path.with_extension("");
        out.push(LocalSshKey {
            name,
            public_key_path: path.to_string_lossy().into_owned(),
            public_key: trimmed.to_string(),
            fingerprint,
            algorithm,
            has_private: private_path.is_file(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Generate a fresh ed25519 keypair and write it to `~/.ssh/<name>`
/// (private, mode 0600 on Unix) plus `~/.ssh/<name>.pub`. Refuses to
/// clobber an existing key — renaming is the operator's call. Returns
/// the same DTO as [`list_local`] so the renderer can immediately
/// surface the new entry without a re-list round-trip.
///
/// The OpenSSH comment is set to `<name>@yah` so the key is
/// identifiable in `authorized_keys` and Hetzner's UI without leaking
/// hostname / username.
pub fn generate(name: &str) -> Result<LocalSshKey, SshKeyError> {
    validate_key_name(name)?;
    let dir = ssh_dir()?;
    fs::create_dir_all(&dir)?;
    set_dir_perms(&dir)?;

    let private_path = dir.join(name);
    let public_path = dir.join(format!("{name}.pub"));
    if private_path.exists() || public_path.exists() {
        return Err(SshKeyError::AlreadyExists(
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
    Ok(LocalSshKey {
        name: name.to_string(),
        public_key_path: public_path.to_string_lossy().into_owned(),
        public_key: openssh_public,
        fingerprint,
        algorithm: private_key.algorithm().as_str().to_string(),
        has_private: true,
    })
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

#[cfg(unix)]
fn set_dir_perms(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_dir_perms(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

// ---------- Tauri commands ----------

#[tauri::command]
pub async fn ssh_key_list_local() -> Result<Vec<LocalSshKey>, String> {
    list_local().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ssh_key_generate(name: String) -> Result<LocalSshKey, String> {
    generate(&name).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_reasonable_names() {
        for n in ["id_ed25519", "yah", "yah-rig", "test_1", "my-key-42"] {
            assert!(validate_key_name(n).is_ok(), "{n} should be valid");
        }
    }

    #[test]
    fn validate_rejects_paths_and_unicode() {
        for n in [
            "",
            "with space",
            "../escape",
            "slash/key",
            "back\\slash",
            "with.dot",
            "ñame",
        ] {
            assert!(validate_key_name(n).is_err(), "{n:?} should be invalid");
        }
    }

    #[test]
    fn generate_round_trips_through_list() {
        // Run inside a per-test temp HOME so we don't touch the user's ~/.ssh.
        let tmp = tempdir_or_skip();
        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &tmp);

        let key = generate("yah_test_key").expect("generate");
        assert_eq!(key.algorithm, "ssh-ed25519");
        assert!(key.has_private);
        assert!(key.public_key.starts_with("ssh-ed25519 "));

        let listed = list_local().expect("list");
        assert!(listed.iter().any(|k| k.name == "yah_test_key"));
        let found = listed.iter().find(|k| k.name == "yah_test_key").unwrap();
        assert_eq!(found.fingerprint, key.fingerprint);

        // Generating again rejects clobber.
        let again = generate("yah_test_key");
        assert!(matches!(again, Err(SshKeyError::AlreadyExists(_, _))));

        // Restore HOME so the rest of the test process is unaffected.
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    fn tempdir_or_skip() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "yah-ssh-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }
}

//! @arch:layer(cli)
//! @arch:role(secrets)
//!
//! Credential vault for `yah keys`. AES-256-GCM at-rest encryption keyed
//! by a per-host random `machine.key`, both files living under
//! `ProjectDirs::data_dir()` (matches `state.rs` convention).
//!
//! Threat model: defends against dragnet disk-image scanners and generic
//! `git grep -i api[_-]key`-style exfil. Does **not** defend against a
//! process running as the same user with FS access — that process can
//! read both the keyfile and the ciphertext blob and decrypt at will.
//! Acceptable on cloud VMs the operator already pays for and trusts;
//! the laptop side keeps using the OS keychain via Tauri's `api_keys`.
//!
//! Layout:
//! - `machine.key`     — 32 raw bytes, mode 0600
//! - `credentials.enc` — `[12-byte nonce | ciphertext_with_tag]`,
//!                        plaintext is `serde_json::Value` map (provider → token)

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, bail, Context, Result};
use directories::ProjectDirs;
use rand::RngCore;
use serde_json::{Map, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MACHINE_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const MACHINE_KEY_FILE: &str = "machine.key";
const CREDENTIALS_FILE: &str = "credentials.enc";

pub struct KeysStore {
    dir: PathBuf,
}

impl KeysStore {
    /// Open the store at the conventional location. Creates the parent
    /// directory if absent (mode 0700 on Unix); does **not** create the
    /// machine key — that's `init`'s job, lazily invoked by `set` so
    /// first-time use Just Works.
    pub fn open() -> Result<Self> {
        let proj = ProjectDirs::from("com", "yah", "yah")
            .context("could not determine yah data directory")?;
        let dir = proj.data_dir().to_path_buf();
        ensure_dir_secure(&dir)?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn machine_key_path(&self) -> PathBuf {
        self.dir.join(MACHINE_KEY_FILE)
    }

    fn credentials_path(&self) -> PathBuf {
        self.dir.join(CREDENTIALS_FILE)
    }

    /// Generate a fresh machine key. Idempotent unless `force` is set:
    /// existing key is preserved (rotating it would orphan any existing
    /// credentials.enc, since this layer doesn't yet do re-encryption).
    pub fn init(&self, force: bool) -> Result<bool> {
        let path = self.machine_key_path();
        if path.exists() && !force {
            return Ok(false);
        }
        let mut key = [0u8; MACHINE_KEY_BYTES];
        rand::thread_rng().fill_bytes(&mut key);
        write_secure(&path, &key)?;
        Ok(true)
    }

    fn load_machine_key(&self) -> Result<[u8; MACHINE_KEY_BYTES]> {
        let path = self.machine_key_path();
        if !path.exists() {
            bail!(
                "machine key missing at {} — run `yah keys init`",
                path.display()
            );
        }
        let mut buf = Vec::new();
        File::open(&path)
            .with_context(|| format!("open {}", path.display()))?
            .read_to_end(&mut buf)?;
        if buf.len() != MACHINE_KEY_BYTES {
            bail!(
                "machine key at {} is {} bytes, expected {}",
                path.display(),
                buf.len(),
                MACHINE_KEY_BYTES
            );
        }
        let mut out = [0u8; MACHINE_KEY_BYTES];
        out.copy_from_slice(&buf);
        Ok(out)
    }

    fn cipher(&self) -> Result<Aes256Gcm> {
        let key = self.load_machine_key()?;
        Aes256Gcm::new_from_slice(&key)
            .map_err(|e| anyhow!("AES key construction failed: {e}"))
    }

    fn read_creds(&self) -> Result<Map<String, Value>> {
        let path = self.credentials_path();
        if !path.exists() {
            return Ok(Map::new());
        }
        let mut blob = Vec::new();
        File::open(&path)
            .with_context(|| format!("open {}", path.display()))?
            .read_to_end(&mut blob)?;
        if blob.len() < NONCE_BYTES + 16 {
            bail!("credentials blob at {} is truncated", path.display());
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_BYTES);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher()?
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow!("decrypt failed — wrong machine key, or credentials.enc corrupted"))?;
        let parsed: Value = serde_json::from_slice(&plaintext)
            .context("decrypted credentials JSON is malformed")?;
        match parsed {
            Value::Object(m) => Ok(m),
            _ => bail!("decrypted credentials are not a JSON object"),
        }
    }

    fn write_creds(&self, creds: &Map<String, Value>) -> Result<()> {
        let plaintext =
            serde_json::to_vec(&Value::Object(creds.clone())).context("serialize creds")?;
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher()?
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|_| anyhow!("encryption failed"))?;
        let mut blob = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);
        write_secure(&self.credentials_path(), &blob)
    }

    pub fn set(&self, provider: &str, token: &str) -> Result<()> {
        validate_provider(provider)?;
        if !self.machine_key_path().exists() {
            self.init(false)?;
        }
        let mut creds = self.read_creds()?;
        creds.insert(provider.to_string(), Value::String(token.to_string()));
        self.write_creds(&creds)
    }

    pub fn get(&self, provider: &str) -> Result<Option<String>> {
        validate_provider(provider)?;
        let creds = self.read_creds()?;
        Ok(creds.get(provider).and_then(|v| v.as_str()).map(str::to_string))
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let creds = self.read_creds()?;
        let mut names: Vec<String> = creds.keys().cloned().collect();
        names.sort();
        Ok(names)
    }

    pub fn delete(&self, provider: &str) -> Result<bool> {
        validate_provider(provider)?;
        let mut creds = self.read_creds()?;
        let removed = creds.remove(provider).is_some();
        if removed {
            self.write_creds(&creds)?;
        }
        Ok(removed)
    }
}

fn validate_provider(name: &str) -> Result<()> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("invalid provider name: {name:?} (use [a-zA-Z0-9_-]+)");
    }
    Ok(())
}

fn ensure_dir_secure(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0700 {}", dir.display()))?;
    }
    Ok(())
}

/// Write `bytes` to `path` atomically with mode 0600 on Unix. Tempfile
/// in the same directory then rename — no half-written secrets visible.
fn write_secure(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("path {} has no parent", path.display()))?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("write")
    ));

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    {
        let mut f = opts
            .open(&tmp)
            .with_context(|| format!("open {}", tmp.display()))?;
        f.write_all(bytes)
            .with_context(|| format!("write {}", tmp.display()))?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store_in(dir: &Path) -> KeysStore {
        ensure_dir_secure(dir).unwrap();
        KeysStore { dir: dir.to_path_buf() }
    }

    #[test]
    fn init_is_idempotent_unless_forced() {
        let tmp = TempDir::new().unwrap();
        let s = store_in(tmp.path());
        assert!(s.init(false).unwrap());
        let key1 = std::fs::read(tmp.path().join("machine.key")).unwrap();
        assert!(!s.init(false).unwrap());
        let key2 = std::fs::read(tmp.path().join("machine.key")).unwrap();
        assert_eq!(key1, key2);
        assert!(s.init(true).unwrap());
        let key3 = std::fs::read(tmp.path().join("machine.key")).unwrap();
        assert_ne!(key1, key3);
    }

    #[test]
    fn roundtrip_and_list() {
        let tmp = TempDir::new().unwrap();
        let s = store_in(tmp.path());
        s.set("anthropic", "sk-ant-test").unwrap();
        s.set("openai", "sk-openai-test").unwrap();
        assert_eq!(s.get("anthropic").unwrap().as_deref(), Some("sk-ant-test"));
        assert_eq!(s.get("openai").unwrap().as_deref(), Some("sk-openai-test"));
        assert_eq!(s.get("missing").unwrap(), None);
        let names = s.list().unwrap();
        assert_eq!(names, vec!["anthropic".to_string(), "openai".to_string()]);
    }

    #[test]
    fn ciphertext_does_not_contain_plaintext() {
        let tmp = TempDir::new().unwrap();
        let s = store_in(tmp.path());
        s.set("anthropic", "sk-ant-DRAGNET-CANARY").unwrap();
        let blob = std::fs::read(tmp.path().join("credentials.enc")).unwrap();
        assert!(!blob.windows(b"sk-ant-DRAGNET-CANARY".len())
            .any(|w| w == b"sk-ant-DRAGNET-CANARY"));
        assert!(!blob.windows(b"anthropic".len())
            .any(|w| w == b"anthropic"));
    }

    #[test]
    fn delete_removes_only_named_provider() {
        let tmp = TempDir::new().unwrap();
        let s = store_in(tmp.path());
        s.set("anthropic", "a").unwrap();
        s.set("openai", "b").unwrap();
        assert!(s.delete("anthropic").unwrap());
        assert!(!s.delete("anthropic").unwrap());
        assert_eq!(s.list().unwrap(), vec!["openai".to_string()]);
    }

    #[test]
    fn wrong_machine_key_fails_decrypt() {
        let tmp = TempDir::new().unwrap();
        let s = store_in(tmp.path());
        s.set("anthropic", "tok").unwrap();
        // Rotate the machine key — existing creds blob now undecryptable.
        s.init(true).unwrap();
        let err = s.get("anthropic").unwrap_err().to_string();
        assert!(err.contains("decrypt"), "expected decrypt error, got: {err}");
    }

    #[test]
    fn rejects_bad_provider_names() {
        let tmp = TempDir::new().unwrap();
        let s = store_in(tmp.path());
        assert!(s.set("", "x").is_err());
        assert!(s.set("has space", "x").is_err());
        assert!(s.set("ok-name_v2", "x").is_ok());
    }
}

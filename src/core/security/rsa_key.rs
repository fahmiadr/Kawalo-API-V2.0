//! RS256 key store with rotation and hot reload, ported from `rsa-key.manager.ts`.
//!
//! Layout of the certs directory, identical to the TypeScript version:
//!
//! ```text
//! certs/private.key        active signing key, PKCS#8 PEM
//! certs/public.key         active public key, SPKI PEM
//! certs/active_kid.txt     key id of the active pair
//! certs/archived/<kid>.pub.pem, <kid>.key   retired pairs
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aws_lc_rs::encoding::AsDer;
use aws_lc_rs::rsa::{KeyPair, KeySize, PublicKey};
use aws_lc_rs::signature::KeyPair as _;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::DecodingKey;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::core::security::token::VerifiedTokens;

/// Required modulus size of generated and accepted keys.
pub const MODULUS_BITS: usize = 4096;

/// Retired keys older than this are no longer published in the JWKS (files stay on disk).
pub const DEFAULT_JWKS_MAX_AGE_DAYS: u64 = 7;

const DEBOUNCE: Duration = Duration::from_millis(300);
const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("certs I/O error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("invalid key material in {path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
    #[error("RSA key generation failed: {0}")]
    Generate(String),
    #[error("private key for kid \"{0}\" not found")]
    NoPrivateKey(String),
    #[error("public key for kid \"{0}\" not found")]
    NoPublicKey(String),
    #[error("file watcher failed: {0}")]
    Watch(#[from] notify::Error),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> KeyError + '_ {
    move |source| KeyError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Public key in JWK form as published at `/.well-known/jwks.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Jwk {
    pub kty: String,
    pub n: String,
    pub e: String,
    pub kid: String,
    #[serde(rename = "use")]
    pub key_use: String,
    pub alg: String,
}

/// JWK Set document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

/// Outcome of [`RsaKeyManager::rotate_key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rotation {
    pub active_kid: String,
    pub retired_kid: String,
}

/// Outcome of [`RsaKeyManager::prune_archived_keys`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub pruned_kids: Vec<String>,
    pub retained_kids: Vec<String>,
}

/// The key that signs new access tokens.
#[derive(Clone)]
pub struct SigningKey {
    pub kid: String,
    pub key: Arc<KeyPair>,
}

#[derive(Clone)]
struct KeyEntry {
    kid: String,
    signing: Option<Arc<KeyPair>>,
    verifying: DecodingKey,
    jwk: Jwk,
    archived_at_ms: Option<u64>,
}

#[derive(Default)]
struct KeyStore {
    active_kid: String,
    entries: Vec<KeyEntry>,
    jwks: Jwks,
}

impl KeyStore {
    fn entry(&self, kid: &str) -> Option<&KeyEntry> {
        self.entries.iter().find(|e| e.kid == kid)
    }

    fn upsert(&mut self, entry: KeyEntry) {
        match self.entries.iter_mut().find(|e| e.kid == entry.kid) {
            Some(slot) => *slot = entry,
            None => self.entries.push(entry),
        }
    }

    /// The active key is always published; retired keys only while younger than `max_age_days`.
    fn rebuild_jwks(&mut self, max_age_days: u64) {
        let now = now_ms();
        let max_age_ms = max_age_days.saturating_mul(MS_PER_DAY);
        self.jwks = Jwks {
            keys: self
                .entries
                .iter()
                .filter(|e| {
                    e.kid == self.active_kid
                        || e.archived_at_ms.map_or(0, |at| now.saturating_sub(at)) <= max_age_ms
                })
                .map(|e| e.jwk.clone())
                .collect(),
        };
    }
}

struct Watching {
    // Dropping the watcher stops the OS subscription.
    _watcher: RecommendedWatcher,
    task: JoinHandle<()>,
}

struct Inner {
    certs_dir: PathBuf,
    modulus_bits: usize,
    store: RwLock<KeyStore>,
    jwks_max_age_days: AtomicU64,
    rotation: Mutex<()>,
    watching: Mutex<Option<Watching>>,
    verified: VerifiedTokens,
}

/// Handle to the shared key store; cloning is cheap.
///
/// Key loading, generation and rotation are blocking (RSA-4096 generation takes seconds): call
/// them from `spawn_blocking` when inside the async runtime.
#[derive(Clone)]
pub struct RsaKeyManager {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for RsaKeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RsaKeyManager")
            .field("certs_dir", &self.inner.certs_dir)
            .field("active_kid", &self.active_kid())
            .finish_non_exhaustive()
    }
}

impl RsaKeyManager {
    /// Loads the key pair in `certs_dir`, generating a 4096-bit pair when missing or too small, and
    /// loads retired public keys from `archived/` for verification.
    ///
    /// # Errors
    /// Fails on I/O errors or unreadable key material.
    pub fn init(certs_dir: impl Into<PathBuf>) -> Result<Self, KeyError> {
        Self::init_with_modulus(certs_dir, MODULUS_BITS)
    }

    /// [`init`](Self::init) with a custom modulus size, so tests need not generate 4096-bit keys.
    ///
    /// # Errors
    /// Same as [`init`](Self::init).
    #[doc(hidden)]
    pub fn init_with_modulus(
        certs_dir: impl Into<PathBuf>,
        modulus_bits: usize,
    ) -> Result<Self, KeyError> {
        let manager = Self {
            inner: Arc::new(Inner {
                certs_dir: certs_dir.into(),
                modulus_bits,
                store: RwLock::new(KeyStore::default()),
                jwks_max_age_days: AtomicU64::new(DEFAULT_JWKS_MAX_AGE_DAYS),
                rotation: Mutex::new(()),
                watching: Mutex::new(None),
                verified: VerifiedTokens::default(),
            }),
        };
        manager.reload()?;
        Ok(manager)
    }

    /// Re-reads the certs directory and atomically replaces the in-memory keys.
    ///
    /// # Errors
    /// Fails on I/O errors or unreadable key material; the previous keys stay active.
    pub fn reload(&self) -> Result<(), KeyError> {
        let mut store = self.inner.load_store()?;
        store.rebuild_jwks(self.jwks_max_age_days());
        *self.inner.write() = store;
        Ok(())
    }

    pub fn active_kid(&self) -> String {
        self.inner.read().active_kid.clone()
    }

    /// Key and `kid` for signing new tokens.
    ///
    /// # Errors
    /// [`KeyError::NoPrivateKey`] when the active key has no private half.
    pub fn signing_key(&self) -> Result<SigningKey, KeyError> {
        let store = self.inner.read();
        store
            .entry(&store.active_kid)
            .and_then(|e| {
                Some(SigningKey {
                    kid: e.kid.clone(),
                    key: Arc::clone(e.signing.as_ref()?),
                })
            })
            .ok_or_else(|| KeyError::NoPrivateKey(store.active_kid.clone()))
    }

    /// Verification key for a token header `kid`. Unknown or absent ids fall back to the active key,
    /// as the TypeScript version does.
    ///
    /// # Errors
    /// [`KeyError::NoPublicKey`] when the store is empty.
    pub fn verifying_key(&self, kid: Option<&str>) -> Result<DecodingKey, KeyError> {
        let store = self.inner.read();
        kid.and_then(|k| store.entry(k))
            .or_else(|| store.entry(&store.active_kid))
            .or_else(|| store.entries.first())
            .map(|e| e.verifying.clone())
            .ok_or_else(|| KeyError::NoPublicKey(kid.unwrap_or(&store.active_kid).to_owned()))
    }

    pub(crate) fn verified_tokens(&self) -> &VerifiedTokens {
        &self.inner.verified
    }

    /// The JWK set to publish.
    pub fn jwks(&self) -> Jwks {
        self.inner.read().jwks.clone()
    }

    pub fn jwks_max_age_days(&self) -> u64 {
        self.inner.jwks_max_age_days.load(Ordering::Relaxed)
    }

    pub fn set_jwks_max_age_days(&self, days: u64) {
        self.inner.jwks_max_age_days.store(days, Ordering::Relaxed);
        self.inner.write().rebuild_jwks(days);
    }

    /// Archives the active pair, generates a new one, activates it and keeps both in the JWKS.
    /// `new_kid` defaults to `kawalo-key-<epoch ms>`.
    ///
    /// # Errors
    /// Fails on I/O errors or key generation failure.
    pub fn rotate_key(&self, new_kid: Option<&str>) -> Result<Rotation, KeyError> {
        let _serial = self.inner.rotation.lock().expect("rotation lock poisoned");
        let dir = &self.inner.certs_dir;
        let archive = dir.join("archived");
        let (private_path, public_path) = (dir.join("private.key"), dir.join("public.key"));

        let retired_kid = self.active_kid();
        let active_kid = new_kid.map_or_else(generate_key_id, str::to_owned);

        create_private_dir(&archive)?;
        if public_path.exists() {
            let target = archive.join(format!("{retired_kid}.pub.pem"));
            fs::copy(&public_path, &target).map_err(io_err(&target))?;
        }
        if private_path.exists() {
            let target = archive.join(format!("{retired_kid}.key"));
            let pem = fs::read(&private_path).map_err(io_err(&private_path))?;
            write_private_file(&target, &pem)?;
        }

        tracing::info!("[RsaKeyManager] rotating key: generating new active key ({active_kid})");
        let (private_pem, public_pem) = generate_pair(self.inner.modulus_bits)?;
        write_private_file(&private_path, private_pem.as_bytes())?;
        fs::write(&public_path, &public_pem).map_err(io_err(&public_path))?;
        let kid_path = dir.join("active_kid.txt");
        fs::write(&kid_path, &active_kid).map_err(io_err(&kid_path))?;

        let entry = KeyEntry::from_pems(
            &active_kid,
            Some(&private_pem),
            &public_pem,
            None,
            &private_path,
        )?;
        {
            let mut store = self.inner.write();
            store.active_kid.clone_from(&active_kid);
            store.upsert(entry);
            if let Some(retired) = store.entries.iter_mut().find(|e| e.kid == retired_kid) {
                retired.archived_at_ms = Some(now_ms());
            }
            store.rebuild_jwks(self.jwks_max_age_days());
        }
        tracing::info!(
            "[RsaKeyManager] key rotation complete. Active kid: {active_kid}, archived kid: {retired_kid}"
        );
        Ok(Rotation {
            active_kid,
            retired_kid,
        })
    }

    /// Deletes archive files older than `max_age_days` (by mtime) and drops their keys from memory.
    ///
    /// # Errors
    /// Fails when the archive directory cannot be listed; per-file errors are logged and skipped.
    pub fn prune_archived_keys(&self, max_age_days: u64) -> Result<PruneReport, KeyError> {
        let archive = self.inner.certs_dir.join("archived");
        if !archive.exists() {
            return Ok(PruneReport::default());
        }
        let now = SystemTime::now();
        let max_age = Duration::from_millis(max_age_days.saturating_mul(MS_PER_DAY));
        let (mut pruned, mut retained) = (Vec::<String>::new(), Vec::<String>::new());

        for entry in fs::read_dir(&archive).map_err(io_err(&archive))?.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let kid = strip_key_suffix(&file_name, &[".pub.pem", ".public.key", ".key"]).to_owned();
            let outcome = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|modified| now.duration_since(modified).unwrap_or_default() > max_age)
                .and_then(|expired| {
                    if expired {
                        fs::remove_file(entry.path())?;
                    }
                    Ok(expired)
                });
            match outcome {
                Ok(true) => push_unique(&mut pruned, kid),
                Ok(false) => push_unique(&mut retained, kid),
                Err(err) => tracing::warn!(
                    "[RsaKeyManager] failed to process or delete archive file {file_name}: {err}"
                ),
            }
        }

        if !pruned.is_empty() {
            let mut store = self.inner.write();
            store
                .entries
                .retain(|e| !pruned.contains(&e.kid) || retained.contains(&e.kid));
            store.rebuild_jwks(self.jwks_max_age_days());
            tracing::info!(
                "[RsaKeyManager] pruning done: {} archived keys older than {max_age_days} days removed",
                pruned.len()
            );
        }
        Ok(PruneReport {
            pruned_kids: pruned,
            retained_kids: retained,
        })
    }

    /// Watches the certs directory and reloads keys 300 ms after `active_kid.txt`, `public.key` or
    /// an archived `*.pub.pem` changes (for example after `rotate_key` ran in another process).
    /// Must be called inside a Tokio runtime. Calling it twice is a no-op.
    ///
    /// # Errors
    /// Fails when the directory cannot be watched.
    pub fn enable_file_watcher(&self) -> Result<(), KeyError> {
        let mut slot = self.inner.watching.lock().expect("watcher lock poisoned");
        if slot.is_some() || !self.inner.certs_dir.exists() {
            return Ok(());
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event
                    && is_key_change(&event)
                {
                    // The receiver only disappears while shutting down.
                    let _ = tx.send(());
                }
            })?;
        watcher.watch(&self.inner.certs_dir, RecursiveMode::NonRecursive)?;

        let inner: Weak<Inner> = Arc::downgrade(&self.inner);
        let task = tokio::spawn(async move {
            while rx.recv().await.is_some() {
                while let Ok(Some(())) = tokio::time::timeout(DEBOUNCE, rx.recv()).await {}
                let Some(inner) = inner.upgrade() else { return };
                let manager = RsaKeyManager { inner };
                tracing::info!(
                    "[RsaKeyManager] certificate change detected on disk, hot-reloading keys"
                );
                match tokio::task::spawn_blocking(move || manager.reload().map(|()| manager)).await
                {
                    Ok(Ok(manager)) => tracing::info!(
                        "[RsaKeyManager] hot-reload succeeded. Active kid: {}",
                        manager.active_kid()
                    ),
                    Ok(Err(err)) => tracing::error!("[RsaKeyManager] hot-reload failed: {err}"),
                    Err(err) => tracing::error!("[RsaKeyManager] hot-reload task failed: {err}"),
                }
            }
        });
        *slot = Some(Watching {
            _watcher: watcher,
            task,
        });
        Ok(())
    }

    /// Stops the file watcher, if running.
    pub fn close_file_watcher(&self) {
        if let Some(watching) = self
            .inner
            .watching
            .lock()
            .expect("watcher lock poisoned")
            .take()
        {
            watching.task.abort();
        }
    }
}

impl Inner {
    fn read(&self) -> std::sync::RwLockReadGuard<'_, KeyStore> {
        self.store.read().expect("key store lock poisoned")
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, KeyStore> {
        // Bumped before the lock so a verification that read the keys earlier cannot be cached
        // afterwards.
        self.verified.invalidate();
        self.store.write().expect("key store lock poisoned")
    }

    fn load_store(&self) -> Result<KeyStore, KeyError> {
        let dir = &self.certs_dir;
        let archive = dir.join("archived");
        let (private_path, public_path) = (dir.join("private.key"), dir.join("public.key"));
        let kid_path = dir.join("active_kid.txt");

        create_private_dir(&archive)?;
        tighten_existing_private_keys(&private_path, &archive);

        let saved_kid = fs::read_to_string(&kid_path)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());

        let needs_new_pair = !private_path.exists()
            || !public_path.exists()
            || fs::read_to_string(&public_path)
                .ok()
                .and_then(|pem| parse_public_key(&pem))
                .is_none_or(|key| key.modulus_len() * 8 != self.modulus_bits);

        let active_kid = saved_kid.clone().unwrap_or_else(generate_key_id);
        if needs_new_pair {
            tracing::info!(
                "[RsaKeyManager] generating new RSA {}-bit keypair ({active_kid}) for RS256 signing",
                self.modulus_bits
            );
            let (private_pem, public_pem) = generate_pair(self.modulus_bits)?;
            write_private_file(&private_path, private_pem.as_bytes())?;
            fs::write(&public_path, public_pem).map_err(io_err(&public_path))?;
        }
        if needs_new_pair || saved_kid.is_none() {
            fs::write(&kid_path, &active_kid).map_err(io_err(&kid_path))?;
        }

        let read = |path: &Path| fs::read_to_string(path).map_err(io_err(path));
        let mut store = KeyStore {
            active_kid: active_kid.clone(),
            ..KeyStore::default()
        };
        store.entries.push(KeyEntry::from_pems(
            &active_kid,
            Some(&read(&private_path)?),
            &read(&public_path)?,
            None,
            &private_path,
        )?);

        let mut archived: Vec<_> = fs::read_dir(&archive)
            .map_err(io_err(&archive))?
            .flatten()
            .map(|e| e.path())
            .collect();
        archived.sort();
        for path in archived {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let kid = strip_key_suffix(&name, &[".pub.pem", ".public.key"]);
            if kid == name {
                continue;
            }
            let loaded = read(&path).and_then(|pem| {
                KeyEntry::from_pems(kid, None, &pem, Some(archived_at_ms(kid, &path)), &path)
            });
            match loaded {
                Ok(entry) => store.upsert(entry),
                Err(err) => {
                    tracing::warn!("[RsaKeyManager] failed to load archived key {name}: {err}")
                }
            }
        }
        Ok(store)
    }
}

impl KeyEntry {
    fn from_pems(
        kid: &str,
        private_pem: Option<&str>,
        public_pem: &str,
        archived_at_ms: Option<u64>,
        origin: &Path,
    ) -> Result<Self, KeyError> {
        let invalid = |reason: String| KeyError::Invalid {
            path: origin.to_owned(),
            reason,
        };
        let public = parse_public_key(public_pem)
            .ok_or_else(|| invalid("not a valid RSA public key PEM".to_owned()))?;
        let (n, e) = (
            URL_SAFE_NO_PAD.encode(public.modulus().big_endian_without_leading_zero()),
            URL_SAFE_NO_PAD.encode(public.exponent().big_endian_without_leading_zero()),
        );
        let signing = private_pem
            .map(|pem| parse_private_key(pem).map(Arc::new).map_err(invalid))
            .transpose()?;
        let verifying =
            DecodingKey::from_rsa_components(&n, &e).map_err(|err| invalid(err.to_string()))?;
        Ok(Self {
            kid: kid.to_owned(),
            signing,
            verifying,
            jwk: Jwk {
                kty: "RSA".to_owned(),
                n,
                e,
                kid: kid.to_owned(),
                key_use: "sig".to_owned(),
                alg: "RS256".to_owned(),
            },
            archived_at_ms,
        })
    }
}

fn parse_public_key(pem: &str) -> Option<PublicKey> {
    let der = pem::parse(pem).ok()?;
    PublicKey::from_der(der.contents()).ok()
}

/// Accepts PKCS#8 (`PRIVATE KEY`, what both servers write) and PKCS#1 (`RSA PRIVATE KEY`).
fn parse_private_key(pem: &str) -> Result<KeyPair, String> {
    let der = pem::parse(pem).map_err(|err| err.to_string())?;
    let key = match der.tag() {
        "RSA PRIVATE KEY" => KeyPair::from_der(der.contents()),
        _ => KeyPair::from_pkcs8(der.contents()),
    };
    key.map_err(|err| err.to_string())
}

fn generate_pair(bits: usize) -> Result<(String, String), KeyError> {
    let generate = |reason: &dyn std::fmt::Display| KeyError::Generate(reason.to_string());
    let size = match bits {
        2048 => KeySize::Rsa2048,
        3072 => KeySize::Rsa3072,
        4096 => KeySize::Rsa4096,
        8192 => KeySize::Rsa8192,
        other => return Err(generate(&format!("unsupported modulus size {other}"))),
    };
    let private = KeyPair::generate(size).map_err(|e| generate(&e))?;
    let private_der = private.as_der().map_err(|e| generate(&e))?;
    let public_der = private.public_key().as_der().map_err(|e| generate(&e))?;
    Ok((
        encode_pem("PRIVATE KEY", private_der.as_ref()),
        encode_pem("PUBLIC KEY", public_der.as_ref()),
    ))
}

fn encode_pem(tag: &str, der: &[u8]) -> String {
    let config = pem::EncodeConfig::new().set_line_ending(pem::LineEnding::LF);
    let mut pem = pem::encode_config(&pem::Pem::new(tag, der), config);
    pem.push('\n');
    pem
}

/// `kawalo-key-<epoch ms>`.
pub fn generate_key_id() -> String {
    format!("kawalo-key-{}", now_ms())
}

/// Archive time of a retired key: the epoch embedded in a `kawalo-key-<ms>` id, else the file mtime.
fn archived_at_ms(kid: &str, path: &Path) -> u64 {
    kid.strip_prefix("kawalo-key-")
        .filter(|ts| !ts.is_empty() && ts.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|ts| ts.parse::<u64>().ok())
        .filter(|&ts| ts > 0)
        .or_else(|| {
            let modified = fs::metadata(path).ok()?.modified().ok()?;
            u64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
        })
        .unwrap_or(0)
}

fn strip_key_suffix<'a>(name: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes
        .iter()
        .find_map(|s| name.strip_suffix(s))
        .unwrap_or(name)
}

fn is_key_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            matches!(name, "active_kid.txt" | "public.key") || name.ends_with(".pub.pem")
        })
}

/// Whether a watcher event means a key file was created, modified or removed.
///
/// `Access` events (open, read, close-after-read) are ignored on purpose: inotify reports them on
/// Linux, and `reload()` itself reads `public.key` and `active_kid.txt`, so reacting to them makes
/// every reload trigger the next one forever. FSEvents on macOS never emits them.
fn is_key_change(event: &notify::Event) -> bool {
    !matches!(event.kind, notify::EventKind::Access(_))
        && event.paths.iter().any(|p| is_key_file(p))
}

fn push_unique(list: &mut Vec<String>, kid: String) {
    if !list.contains(&kid) {
        list.push(kid);
    }
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

/// Creates `dir` (and parents) readable only by the owner; existing directories are left as is.
fn create_private_dir(dir: &Path) -> Result<(), KeyError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io_err(dir))
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir).map_err(io_err(dir))
    }
}

/// Writes private key material owner-only. `mode` on `OpenOptions` applies only when the file is
/// created, so permissions are set again to also cover a file that already existed.
fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), KeyError> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(io_err(path))?;
        file.write_all(contents).map_err(io_err(path))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io_err(path))
    }
    #[cfg(not(unix))]
    {
        fs::write(path, contents).map_err(io_err(path))
    }
}

/// Keys written by older code (including the TypeScript server) may be group/world readable.
fn tighten_existing_private_keys(private_path: &Path, archive: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let archived = fs::read_dir(archive)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "key"));
        for path in std::iter::once(private_path.to_path_buf()).chain(archived) {
            if path.exists()
                && let Err(err) = fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            {
                tracing::warn!(
                    "[RsaKeyManager] could not restrict permissions of {}: {err}",
                    path.display()
                );
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (private_path, archive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_BITS: usize = 2048;

    fn manager(dir: &Path) -> RsaKeyManager {
        RsaKeyManager::init_with_modulus(dir, TEST_BITS).expect("init")
    }

    fn event(kind: notify::EventKind, path: &str) -> notify::Event {
        notify::Event::new(kind).add_path(PathBuf::from(path))
    }

    #[test]
    fn watcher_ignores_reads_of_key_files() {
        use notify::EventKind;
        use notify::event::{AccessKind, AccessMode};

        for kind in [
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            EventKind::Access(AccessKind::Read),
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
        ] {
            assert!(!is_key_change(&event(kind, "certs/public.key")));
            assert!(!is_key_change(&event(kind, "certs/active_kid.txt")));
        }
    }

    #[test]
    fn watcher_reacts_to_writes_of_key_files_only() {
        use notify::EventKind;
        use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind};

        let modified = EventKind::Modify(ModifyKind::Data(DataChange::Any));
        assert!(is_key_change(&event(modified, "certs/active_kid.txt")));
        assert!(is_key_change(&event(modified, "certs/public.key")));
        assert!(is_key_change(&event(
            EventKind::Create(CreateKind::File),
            "certs/archived/kawalo-key-1.pub.pem"
        )));
        assert!(is_key_change(&event(
            EventKind::Remove(RemoveKind::File),
            "certs/archived/kawalo-key-1.pub.pem"
        )));
        assert!(!is_key_change(&event(modified, "certs/private.key")));
    }

    #[test]
    fn first_init_generates_files_and_publishes_one_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = manager(dir.path());
        let kid = keys.active_kid();
        assert!(kid.starts_with("kawalo-key-"), "{kid}");
        assert_eq!(
            fs::read_to_string(dir.path().join("active_kid.txt")).expect("kid file"),
            kid
        );
        let jwks = keys.jwks();
        assert_eq!(jwks.keys.len(), 1);
        let jwk = &jwks.keys[0];
        assert_eq!(
            (jwk.kty.as_str(), jwk.alg.as_str(), jwk.key_use.as_str()),
            ("RSA", "RS256", "sig")
        );
        assert_eq!(jwk.kid, kid);
        assert!(keys.signing_key().is_ok());
    }

    #[test]
    fn pkcs1_private_keys_are_accepted() {
        let (private_pem, public_pem) = generate_pair(TEST_BITS).expect("pair");
        let pkcs8 = pem::parse(&private_pem).expect("pem");
        // PrivateKeyInfo wraps the PKCS#1 key in an OCTET STRING with a two-byte long-form length.
        let rsa_oid = [
            0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
        ];
        let at = pkcs8
            .contents()
            .windows(rsa_oid.len())
            .position(|w| w == rsa_oid)
            .expect("rsaEncryption algorithm id");
        let pkcs1 = &pkcs8.contents()[at + rsa_oid.len() + 4..];
        assert_eq!(&pkcs8.contents()[at + rsa_oid.len()..][..2], [0x04, 0x82]);
        let pkcs1_pem = encode_pem("RSA PRIVATE KEY", pkcs1);

        let from_pkcs1 =
            KeyEntry::from_pems("k", Some(&pkcs1_pem), &public_pem, None, Path::new("t"))
                .expect("pkcs1");
        let from_pkcs8 =
            KeyEntry::from_pems("k", Some(&private_pem), &public_pem, None, Path::new("t"))
                .expect("pkcs8");
        assert_eq!(from_pkcs1.jwk, from_pkcs8.jwk);
        assert!(from_pkcs1.signing.is_some());
    }

    #[test]
    fn second_init_reuses_the_stored_pair() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = manager(dir.path()).jwks();
        assert_eq!(manager(dir.path()).jwks(), first);
    }

    #[test]
    fn wrong_modulus_triggers_regeneration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = manager(dir.path()).jwks().keys[0].n.clone();
        let after = RsaKeyManager::init_with_modulus(dir.path(), 3072)
            .expect("init")
            .jwks()
            .keys[0]
            .n
            .clone();
        assert_ne!(before, after);
    }

    #[test]
    fn rotation_archives_old_key_and_keeps_both_in_jwks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = manager(dir.path());
        let old = keys.active_kid();
        let rotation = keys.rotate_key(Some("kawalo-key-42")).expect("rotate");
        assert_eq!(
            rotation,
            Rotation {
                active_kid: "kawalo-key-42".into(),
                retired_kid: old.clone()
            }
        );
        assert!(
            dir.path()
                .join("archived")
                .join(format!("{old}.pub.pem"))
                .exists()
        );
        assert!(
            dir.path()
                .join("archived")
                .join(format!("{old}.key"))
                .exists()
        );
        let kids: Vec<_> = keys.jwks().keys.into_iter().map(|k| k.kid).collect();
        assert_eq!(kids, vec![old.clone(), "kawalo-key-42".to_owned()]);
        assert_eq!(keys.signing_key().expect("signing").kid, "kawalo-key-42");
        assert!(keys.verifying_key(Some(&old)).is_ok());

        let reloaded = manager(dir.path());
        assert_eq!(reloaded.active_kid(), "kawalo-key-42");
        assert_eq!(reloaded.jwks().keys.len(), 2);
    }

    #[test]
    fn old_archived_keys_leave_the_jwks_but_stay_loaded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = manager(dir.path());
        let old = keys.active_kid();
        keys.rotate_key(Some("kawalo-key-5")).expect("rotate");
        // A kid carrying a tiny epoch reads as archived decades ago.
        let archived = dir.path().join("archived");
        fs::rename(
            archived.join(format!("{old}.pub.pem")),
            archived.join("kawalo-key-1.pub.pem"),
        )
        .expect("rename");
        let reloaded = manager(dir.path());
        assert!(reloaded.jwks().keys.iter().all(|k| k.kid != "kawalo-key-1"));
        assert!(reloaded.inner.read().entry("kawalo-key-1").is_some());
    }

    #[test]
    fn prune_removes_only_expired_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = manager(dir.path());
        let old = keys.active_kid();
        keys.rotate_key(Some("kawalo-key-7")).expect("rotate");
        let archived = dir.path().join("archived");

        let report = keys.prune_archived_keys(30).expect("prune");
        assert_eq!(report.pruned_kids, Vec::<String>::new());
        assert_eq!(report.retained_kids, vec![old.clone()]);

        let forty_days_ago = SystemTime::now() - Duration::from_secs(40 * 24 * 3600);
        for name in [format!("{old}.pub.pem"), format!("{old}.key")] {
            fs::File::options()
                .write(true)
                .open(archived.join(name))
                .and_then(|f| f.set_modified(forty_days_ago))
                .expect("backdate");
        }
        let report = keys.prune_archived_keys(30).expect("prune");
        assert_eq!(report.pruned_kids, vec![old.clone()]);
        assert!(!archived.join(format!("{old}.pub.pem")).exists());
        assert!(keys.inner.read().entry(&old).is_none());
        assert_eq!(keys.jwks().keys.len(), 1);
    }

    #[test]
    fn unknown_kid_falls_back_to_the_active_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = manager(dir.path());
        assert!(keys.verifying_key(Some("does-not-exist")).is_ok());
        assert!(keys.verifying_key(None).is_ok());
    }

    #[tokio::test]
    async fn watcher_hot_reloads_after_external_rotation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let serving = manager(dir.path());
        serving.enable_file_watcher().expect("watch");
        let before = serving.active_kid();

        let cli = manager(dir.path());
        cli.rotate_key(Some("kawalo-key-hot")).expect("rotate");

        let mut reloaded = false;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if serving.active_kid() == "kawalo-key-hot" {
                reloaded = true;
                break;
            }
        }
        serving.close_file_watcher();
        assert!(reloaded, "still on {before}");
    }

    #[cfg(unix)]
    #[test]
    fn private_key_files_and_dirs_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let certs = dir.path().join("certs");
        let manager = RsaKeyManager::init_with_modulus(&certs, 2048).expect("init");
        manager.rotate_key(None).expect("rotate");

        let mode = |p: &Path| fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&certs.join("private.key")), 0o600);
        assert_eq!(mode(&certs.join("archived")), 0o700);
        let archived_keys: Vec<_> = fs::read_dir(certs.join("archived"))
            .expect("read archive")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "key"))
            .collect();
        assert!(!archived_keys.is_empty());
        for key in archived_keys {
            assert_eq!(mode(&key), 0o600);
        }
    }

    #[cfg(unix)]
    #[test]
    fn startup_tightens_keys_created_with_loose_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let certs = dir.path().join("certs");
        RsaKeyManager::init_with_modulus(&certs, 2048).expect("init");
        let private = certs.join("private.key");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o644)).expect("loosen");

        RsaKeyManager::init_with_modulus(&certs, 2048).expect("reinit");
        let mode = fs::metadata(&private)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

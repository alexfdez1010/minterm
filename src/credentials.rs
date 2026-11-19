use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const ACTIVE: &str = "credentials";
const STAGING: &str = ".credential-stage-";
const MASTER: &str = "master.key";
const VAULT: &str = "keys.enc";
const VERSION: &[u8; 4] = b"MTK1";
const NONCE_BYTES: usize = 24;
const AAD: &[u8] = b"Minterm local credentials v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAi,
    Anthropic,
}

impl Provider {
    fn env(self) -> &'static str {
        match self {
            Self::OpenAi => "OPEN_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Keys {
    openai: Option<String>,
    anthropic: Option<String>,
}

impl Keys {
    fn entry(&mut self, provider: Provider) -> &mut Option<String> {
        match provider {
            Provider::OpenAi => &mut self.openai,
            Provider::Anthropic => &mut self.anthropic,
        }
    }
}

pub fn rotate_on_launch() -> Result<(), String> {
    rotate(&root()?)
}

pub fn save(provider: Provider, key: &str) -> Result<(), String> {
    update(&root()?, provider, Some(valid_key(key)?.to_owned()))
}

pub fn remove(provider: Provider) -> Result<(), String> {
    update(&root()?, provider, None)
}

pub fn lookup(provider: Provider) -> Result<String, String> {
    configured_key(provider)?.ok_or_else(|| {
        format!(
            "Save an API key in Minterm or set {} before launching.",
            provider.env()
        )
    })
}

pub fn configured_key(provider: Provider) -> Result<Option<String>, String> {
    let saved = locked(&root()?, |root| {
        Ok(load(root)?.unwrap_or_default().entry(provider).take())
    })?;
    saved
        .or_else(|| std::env::var(provider.env()).ok())
        .filter(|key| !key.trim().is_empty())
        .map(|key| valid_key(&key).map(str::to_owned))
        .transpose()
}

pub enum Source {
    Saved,
    Environment(&'static str),
    Missing,
}

pub fn source(provider: Provider) -> Result<Source, String> {
    let saved = locked(&root()?, |root| {
        Ok(load(root)?.unwrap_or_default().entry(provider).is_some())
    })?;
    if saved {
        Ok(Source::Saved)
    } else if std::env::var(provider.env()).is_ok_and(|key| valid_key(&key).is_ok()) {
        Ok(Source::Environment(provider.env()))
    } else {
        Ok(Source::Missing)
    }
}

fn root() -> Result<PathBuf, String> {
    std::env::home_dir()
        .map(|home| home.join("Library/Application Support/Minterm"))
        .ok_or_else(|| "Could not locate your home directory.".into())
}

fn valid_key(key: &str) -> Result<&str, String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 4096 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Enter a valid API key without spaces or line breaks.".into());
    }
    Ok(key)
}

fn rotate(root: &Path) -> Result<(), String> {
    locked(root, |root| {
        if let Some(keys) = load(root)? {
            commit(root, &keys)?;
        }
        Ok(())
    })
}

fn update(root: &Path, provider: Provider, key: Option<String>) -> Result<(), String> {
    locked(root, |root| {
        let mut keys = load(root)?.unwrap_or_default();
        if key.is_none() && keys.entry(provider).is_none() {
            return Ok(());
        }
        *keys.entry(provider) = key;
        commit(root, &keys)
    })
}

fn locked<T>(root: &Path, operation: impl FnOnce(&Path) -> io::Result<T>) -> Result<T, String> {
    let run = || {
        DirBuilder::new().recursive(true).mode(0o700).create(root)?;
        check_private(root.symlink_metadata()?, true)?;
        // Keep the lock outside both generations so all app instances share an inode.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join("credentials.lock"))?;
        check_private(lock.metadata()?, false)?;
        lock.lock()?;
        operation(root)
    };
    run().map_err(|e| format!("Could not access saved API keys. {e}"))
}

fn check_private(metadata: Metadata, directory: bool) -> io::Result<()> {
    let expected_type = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    // SAFETY: geteuid has no arguments or memory requirements.
    if !expected_type
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "Check ownership and owner-only permissions in Application Support/Minterm.",
        ));
    }
    Ok(())
}

fn read_private(path: &Path, max: usize) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    check_private(file.metadata()?, false)?;
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(damaged());
    }
    Ok(bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn damaged() -> io::Error {
    io::Error::new(
        ErrorKind::InvalidData,
        "The saved credentials could not be authenticated.",
    )
}

fn load(root: &Path) -> io::Result<Option<Keys>> {
    let active = root.join(ACTIVE);
    let pending = stages(root)?;
    let keys = match active.symlink_metadata() {
        Ok(metadata) => {
            check_private(metadata, true)?;
            read_keys(&active)?
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            // A crash can leave the first save on disk just before publication.
            // A master without a vault contains no user credentials. A vault
            // with missing or invalid key material must always be preserved.
            let mut recoverable = None;
            for dir in &pending {
                check_private(dir.symlink_metadata()?, true)?;
                if dir
                    .join(VAULT)
                    .symlink_metadata()
                    .is_err_and(|e| e.kind() == ErrorKind::NotFound)
                {
                    fs::remove_dir_all(dir)?;
                    continue;
                }
                if recoverable.is_some() {
                    return Err(damaged());
                }
                recoverable = Some((dir, read_keys(dir)?));
            }
            let Some((staged, keys)) = recoverable else {
                return Ok(None);
            };
            fs::rename(staged, &active)?;
            File::open(root)?.sync_all()?;
            return Ok(Some(keys));
        }
        Err(error) => return Err(error),
    };
    // After a crash following a swap, these directories contain old generations.
    for stage in pending {
        fs::remove_dir_all(stage)?;
    }
    Ok(Some(keys))
}

fn read_keys(active: &Path) -> io::Result<Keys> {
    let key = read_private(&active.join(MASTER), 32)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| damaged())?;
    let bytes = read_private(&active.join(VAULT), 64 * 1024)?;
    if bytes.len() < VERSION.len() + NONCE_BYTES || !bytes.starts_with(VERSION) {
        return Err(damaged());
    }
    let (nonce, ciphertext) = bytes[VERSION.len()..].split_at(NONCE_BYTES);
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: AAD,
            },
        )
        .map_err(|_| damaged())?;
    serde_json::from_slice(&plaintext).map_err(|_| damaged())
}

fn stages(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut stages = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .file_name()
            .as_encoded_bytes()
            .starts_with(STAGING.as_bytes())
            && entry.file_type()?.is_dir()
        {
            stages.push(entry.path());
        }
    }
    Ok(stages)
}

fn commit(root: &Path, keys: &Keys) -> io::Result<()> {
    let mut key = [0_u8; 32];
    let mut nonce = [0_u8; NONCE_BYTES];
    getrandom::fill(&mut key)
        .map_err(|_| io::Error::other("Could not generate a new encryption key."))?;
    getrandom::fill(&mut nonce)
        .map_err(|_| io::Error::other("Could not generate an encryption nonce."))?;
    let plaintext = serde_json::to_vec(keys)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let encrypted = cipher
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: &plaintext,
                aad: AAD,
            },
        )
        .map_err(|_| io::Error::other("Could not encrypt credentials."))?;

    let staged = tempfile::Builder::new()
        .prefix(STAGING)
        .permissions(Permissions::from_mode(0o700))
        .tempdir_in(root)?;
    write_private(&staged.path().join(MASTER), &key)?;
    write_private(
        &staged.path().join(VAULT),
        &[VERSION.as_slice(), &nonce, &encrypted].concat(),
    )?;
    File::open(staged.path())?.sync_all()?;

    let active = root.join(ACTIVE);
    if active.try_exists()? {
        let staged_path = CString::new(staged.path().as_os_str().as_bytes())?;
        let active_path = CString::new(active.as_os_str().as_bytes())?;
        // SAFETY: both NUL-terminated paths remain live through this call.
        // RENAME_SWAP atomically swaps directories on the same macOS volume.
        if unsafe {
            libc::renamex_np(
                staged_path.as_ptr(),
                active_path.as_ptr(),
                libc::RENAME_SWAP,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        File::open(root)?.sync_all()?;
        staged.close()?;
    } else {
        fs::rename(staged.path(), active)?;
        File::open(root)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_preserves_both_providers_with_new_key_and_ciphertext() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Minterm");
        rotate(&root).unwrap();
        assert!(!root.join(ACTIVE).exists());
        update(&root, Provider::OpenAi, Some("test-openai".into())).unwrap();
        update(&root, Provider::Anthropic, Some("test-anthropic".into())).unwrap();
        let master = root.join(ACTIVE).join(MASTER);
        let vault = root.join(ACTIVE).join(VAULT);
        let previous_key = fs::read(&master).unwrap();
        let previous_vault = fs::read(&vault).unwrap();
        assert!(!previous_vault.windows(4).any(|s| s == b"test"));
        rotate(&root).unwrap();
        assert_ne!(fs::read(&master).unwrap(), previous_key);
        assert_ne!(fs::read(&vault).unwrap(), previous_vault);
        assert_eq!(master.metadata().unwrap().mode() & 0o777, 0o600);
        assert_eq!(vault.metadata().unwrap().mode() & 0o777, 0o600);
        assert_eq!(root.metadata().unwrap().mode() & 0o777, 0o700);
        let keys = load(&root).unwrap().unwrap();
        assert_eq!(keys.openai.as_deref(), Some("test-openai"));
        assert_eq!(keys.anthropic.as_deref(), Some("test-anthropic"));
        update(&root, Provider::OpenAi, None).unwrap();
        assert!(load(&root).unwrap().unwrap().openai.is_none());
    }

    #[test]
    fn damaged_or_missing_files_never_trigger_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Minterm");
        update(&root, Provider::OpenAi, Some("example".into())).unwrap();
        let master = root.join(ACTIVE).join(MASTER);
        let vault = root.join(ACTIVE).join(VAULT);
        let original = fs::read(&master).unwrap();
        let mut damaged = fs::read(&vault).unwrap();
        *damaged.last_mut().unwrap() ^= 1;
        fs::write(&vault, &damaged).unwrap();
        assert!(rotate(&root).is_err());
        assert_eq!(fs::read(&master).unwrap(), original);
        fs::remove_file(&master).unwrap();
        assert!(update(&root, Provider::Anthropic, Some("other".into())).is_err());
        assert!(!master.exists());
        assert_eq!(fs::read(vault).unwrap(), damaged);
    }

    #[test]
    fn first_save_recovers_after_interrupted_publication() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Minterm");
        update(&root, Provider::OpenAi, Some("kept".into())).unwrap();
        fs::rename(
            root.join(ACTIVE),
            root.join(format!("{STAGING}interrupted")),
        )
        .unwrap();
        rotate(&root).unwrap();
        assert_eq!(
            load(&root).unwrap().unwrap().openai.as_deref(),
            Some("kept")
        );
        assert!(stages(&root).unwrap().is_empty());
    }

    #[test]
    fn concurrent_instances_preserve_updates_to_separate_providers() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Minterm");
        std::thread::scope(|scope| {
            scope.spawn(|| update(&root, Provider::OpenAi, Some("first".into())).unwrap());
            scope.spawn(|| update(&root, Provider::Anthropic, Some("second".into())).unwrap());
            scope.spawn(|| rotate(&root).unwrap());
        });
        let keys = load(&root).unwrap().unwrap();
        assert_eq!(keys.openai.as_deref(), Some("first"));
        assert_eq!(keys.anthropic.as_deref(), Some("second"));
    }

    #[test]
    fn validates_keys_and_provider_wire_names() {
        assert_eq!(valid_key("  example\n").unwrap(), "example");
        for key in ["", "a b", "key\r\nheader", "\0"] {
            assert!(valid_key(key).is_err());
        }
        assert_eq!(serde_json::to_value(Provider::OpenAi).unwrap(), "openai");
        assert_eq!(
            serde_json::to_value(Provider::Anthropic).unwrap(),
            "anthropic"
        );
    }
}

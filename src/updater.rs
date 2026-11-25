//! Startup-only updates. Downloads and verification run off the event loop;
//! a detached copy of this executable swaps bundles after the app exits.
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::CString,
    fs,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const NS: &str = "http://www.andymatuschak.org/xml-namespaces/sparkle";
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Serialize)]
pub struct Progress {
    pub status: &'static str,
    pub detail: String,
    pub fraction: Option<f64>,
}

struct Release {
    version: u64,
    display: String,
    url: String,
    length: u64,
    signature: String,
}

#[derive(Serialize, Deserialize)]
struct Plan {
    target: PathBuf,
    directory: PathBuf,
    parent_pid: u32,
}

pub struct Prepared {
    stage: TempDir,
    target: PathBuf,
}

fn field(info: &plist::Value, key: &str) -> Result<String> {
    info.as_dictionary()
        .and_then(|d| d.get(key))
        .and_then(|v| v.as_string())
        .map(str::to_owned)
        .ok_or_else(|| format!("Missing bundle field: {key}").into())
}

fn info(app: &Path) -> Result<plist::Value> {
    Ok(plist::Value::from_file(app.join("Contents/Info.plist"))?)
}

pub fn installed_bundle() -> Option<PathBuf> {
    let executable = fs::canonicalize(env::current_exe().ok()?).ok()?;
    let app = executable.parent()?.parent()?.parent()?;
    (app.extension().is_some_and(|ext| ext == "app")
        && executable == app.join("Contents/MacOS/minterm"))
    .then(|| app.to_owned())
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("Update canceled".into())
    } else {
        Ok(())
    }
}

fn validate_url(url: &str) -> Result<()> {
    let uri: ureq::http::Uri = url.parse()?;
    if uri.scheme_str() == Some("https")
        || (uri.scheme_str() == Some("http")
            && matches!(uri.host(), Some("127.0.0.1" | "localhost" | "[::1]")))
    {
        Ok(())
    } else {
        Err("Update downloads require HTTPS".into())
    }
}

fn release(xml: &str, current: u64) -> Result<Option<Release>> {
    let document = roxmltree::Document::parse(xml)?;
    let mut releases = Vec::new();
    for item in document.descendants().filter(|n| n.has_tag_name("item")) {
        let text = |name| {
            item.children()
                .find(|n| n.has_tag_name((NS, name)))
                .and_then(|n| n.text())
        };
        let version: u64 = text("version").ok_or("Missing update version")?.parse()?;
        if version <= current {
            continue;
        }
        let enclosure = item
            .children()
            .find(|n| n.has_tag_name("enclosure"))
            .ok_or("Missing update download")?;
        let length = enclosure
            .attribute("length")
            .ok_or("Missing update length")?
            .parse()?;
        if length == 0 || length > MAX_DOWNLOAD {
            return Err("Update is too large".into());
        }
        let url = enclosure
            .attribute("url")
            .ok_or("Missing update URL")?
            .to_owned();
        validate_url(&url)?;
        releases.push(Release {
            version,
            display: text("shortVersionString")
                .unwrap_or("new version")
                .to_owned(),
            url,
            length,
            signature: enclosure
                .attribute((NS, "edSignature"))
                .ok_or("Missing update signature")?
                .to_owned(),
        });
    }
    Ok(releases.into_iter().max_by_key(|r| r.version))
}

fn verify(bytes: &[u8], key: &str, signature: &str) -> Result<()> {
    let key: [u8; 32] = STANDARD
        .decode(key)?
        .try_into()
        .map_err(|_| "Invalid update public key")?;
    let signature = Signature::from_slice(&STANDARD.decode(signature)?)?;
    VerifyingKey::from_bytes(&key)?.verify_strict(bytes, &signature)?;
    Ok(())
}

fn agent(seconds: u64, https: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .https_only(https)
        .build()
        .into()
}

pub fn prepare(
    target: PathBuf,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Progress),
) -> Result<Option<Prepared>> {
    let host = info(&target)?;
    let current: u64 = field(&host, "CFBundleVersion")?.parse()?;
    let feed_url = field(&host, "SUFeedURL")?;
    validate_url(&feed_url)?;
    let https = feed_url.starts_with("https:");
    let xml = agent(10, https)
        .get(&feed_url)
        .call()?
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_string()?;
    check_cancel(&cancel)?;
    let Some(release) = release(&xml, current)? else {
        return Ok(None);
    };
    let response = agent(180, https).get(&release.url).call()?;
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    let mut chunk = [0; 64 * 1024];
    let mut last_progress = Instant::now() - Duration::from_secs(1);
    loop {
        check_cancel(&cancel)?;
        let length = reader.read(&mut chunk)?;
        if length == 0 {
            break;
        }
        if bytes.len() as u64 + length as u64 > release.length {
            return Err("Update download exceeds its declared size".into());
        }
        bytes.extend_from_slice(&chunk[..length]);
        if last_progress.elapsed() >= Duration::from_millis(100) {
            progress(Progress {
                status: "Downloading update",
                detail: format!("Version {}", release.display),
                fraction: Some(bytes.len() as f64 / release.length as f64),
            });
            last_progress = Instant::now();
        }
    }
    if bytes.len() as u64 != release.length {
        return Err("The update download is incomplete".into());
    }
    progress(Progress {
        status: "Verifying update",
        detail: String::new(),
        fraction: None,
    });
    // The feed is only discovery metadata. The archive itself must be signed,
    // and its signed bundle must match the advertised, strictly newer version.
    verify(&bytes, &field(&host, "SUPublicEDKey")?, &release.signature)
        .map_err(|_| "The downloaded update could not be verified")?;
    check_cancel(&cancel)?;
    let stage = tempfile::Builder::new()
        .prefix(".minterm-update-")
        .tempdir_in(target.parent().ok_or("Missing app folder")?)?;
    let dmg = stage.path().join("update.dmg");
    fs::write(&dmg, bytes)?;
    progress(Progress {
        status: "Preparing update",
        detail: String::new(),
        fraction: None,
    });
    let mount = Mounted::open(&dmg)?;
    let source = mount.0.path().join("Minterm.app");
    let replacement = info(&source)?;
    if field(&replacement, "CFBundleIdentifier")? != field(&host, "CFBundleIdentifier")?
        || field(&replacement, "CFBundleExecutable")? != "minterm"
        || field(&replacement, "CFBundleVersion")?.parse::<u64>()? != release.version
    {
        return Err("The downloaded app does not match the update".into());
    }
    let system = plist::Value::from_file("/System/Library/CoreServices/SystemVersion.plist")?;
    if os_version(&field(&replacement, "LSMinimumSystemVersion")?)?
        > os_version(&field(&system, "ProductVersion")?)?
    {
        return Err("This update requires a newer version of macOS".into());
    }
    run(Command::new("/usr/bin/ditto")
        .arg("--noqtn")
        .arg(&source)
        .arg(stage.path().join("Minterm.app")))?;
    drop(mount);
    let app = stage.path().join("Minterm.app");
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app))?;
    let arch = if env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        env::consts::ARCH
    };
    run(Command::new("/usr/bin/lipo")
        .args(["-verify_arch", arch])
        .arg(app.join("Contents/MacOS/minterm")))?;
    check_cancel(&cancel)?;
    fs::remove_file(dmg)?;
    Ok(Some(Prepared { stage, target }))
}

// System tools only handle macOS disk images and signed bundles. They never
// receive shell source, and a stalled tool cannot hold startup indefinitely.
fn run(command: &mut Command) -> Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("Update tool failed: {:?}", command.get_program()).into())
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("An update tool timed out".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct Mounted(TempDir);
impl Mounted {
    fn open(dmg: &Path) -> Result<Self> {
        let mount = Self(tempfile::tempdir()?);
        run(Command::new("/usr/bin/hdiutil")
            .args(["attach", "-readonly", "-nobrowse", "-quiet", "-mountpoint"])
            .arg(mount.0.path())
            .arg(dmg))?;
        Ok(mount)
    }
}
impl Drop for Mounted {
    fn drop(&mut self) {
        let _ = run(Command::new("/usr/bin/hdiutil")
            .arg("detach")
            .arg(self.0.path())
            .args(["-force", "-quiet"]));
    }
}

impl Prepared {
    pub fn install(self, directory: PathBuf) -> Result<()> {
        let plan = Plan {
            target: self.target,
            directory,
            parent_pid: std::process::id(),
        };
        let path = self.stage.path();
        fs::write(path.join("plan.json"), serde_json::to_vec(&plan)?)?;
        // A separate copy keeps the helper alive while the old bundle is moved.
        fs::copy(env::current_exe()?, path.join("installer"))?;
        let log = fs::File::create(path.join("install.log"))?;
        Command::new(path.join("installer"))
            .arg("--apply-update")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        let _ = self.stage.keep();
        Ok(())
    }
}

fn reopen(target: &Path, directory: &Path) -> Result<()> {
    run(Command::new("/usr/bin/open")
        .arg("-n")
        .arg(target)
        .args(["--args", "--skip-update"])
        .arg(directory))
}

fn os_version(value: &str) -> Result<[u32; 3]> {
    let mut version = [0; 3];
    for (index, part) in value.split('.').enumerate() {
        *version.get_mut(index).ok_or("Invalid macOS version")? = part.parse()?;
    }
    Ok(version)
}

fn exchange(first: &Path, second: &Path) -> Result<()> {
    let first = CString::new(first.as_os_str().as_bytes())?;
    let second = CString::new(second.as_os_str().as_bytes())?;
    // SAFETY: both paths are live, NUL-terminated C strings. RENAME_SWAP
    // atomically exchanges bundles on the same volume, keeping the old copy.
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            first.as_ptr(),
            libc::AT_FDCWD,
            second.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

fn swap(stage: &Path, plan: &Plan) -> Result<()> {
    let new = stage.join("Minterm.app");
    exchange(&new, &plan.target)?;
    if let Err(error) = reopen(&plan.target, &plan.directory) {
        exchange(&new, &plan.target)?;
        return Err(error);
    }
    Ok(())
}

pub fn apply_if_requested() -> std::result::Result<bool, String> {
    fn apply(stage: &Path) -> Result<()> {
        let stage = fs::canonicalize(stage)?;
        if fs::canonicalize(env::current_exe()?)? != stage.join("installer") {
            return Err("The update helper must run from its staging folder".into());
        }
        let plan: Plan = serde_json::from_slice(&fs::read(stage.join("plan.json"))?)?;
        if stage.parent() != plan.target.parent() || !plan.target.is_dir() {
            return Err("Invalid update destination".into());
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        // SAFETY: signal zero only checks whether the original process exists.
        while unsafe { libc::kill(plan.parent_pid as i32, 0) } == 0 {
            if Instant::now() >= deadline {
                return Err("Minterm did not exit for its update".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
        if let Err(error) = swap(&stage, &plan) {
            let _ = reopen(&plan.target, &plan.directory);
            return Err(error);
        }
        fs::remove_dir_all(stage)?;
        Ok(())
    }
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--apply-update")) {
        return Ok(false);
    }
    let path = args.next().ok_or("Missing update staging folder")?;
    if args.next().is_some() {
        return Err("Unexpected update helper arguments".into());
    }
    apply(Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn only_the_signed_archive_is_accepted() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let archive = b"a signed disk image";
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        let signature = STANDARD.encode(key.sign(archive).to_bytes());
        assert!(verify(archive, &public, &signature).is_ok());
        assert!(verify(b"a modified disk image", &public, &signature).is_err());
        assert!(verify(archive, &public, &STANDARD.encode([0; 64])).is_err());
        assert!(verify(archive, "invalid", &signature).is_err());
        // Generated by Sparkle 2.10.1 sign_update with the same test seed.
        let legacy = "cdLlzONnzbj6M7UKax5Tqi0AqJW+IlrBouXwEBJRKtLdATY4cNsPuK210qb0QfHzMg3UOn93/9vQk3Pwty/zAA==";
        assert!(verify(archive, &public, legacy).is_ok());
    }

    #[test]
    fn feed_rejects_downgrades_oversized_and_insecure_downloads() {
        let feed = |version, length, url| {
            format!(
                r#"<rss xmlns:s="{NS}"><channel><item>
            <s:version>{version}</s:version><s:shortVersionString>1.2.3</s:shortVersionString>
            <enclosure url="{url}" length="{length}" s:edSignature="signature"/>
            </item></channel></rss>"#
            )
        };
        assert!(
            release(&feed(3, 20, "https://github.com/update.dmg"), 3)
                .unwrap()
                .is_none()
        );
        assert!(
            release(&feed(2, 20, "https://github.com/update.dmg"), 3)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            release(&feed(4, 20, "https://github.com/update.dmg"), 3)
                .unwrap()
                .unwrap()
                .version,
            4
        );
        assert!(
            release(
                &feed(4, MAX_DOWNLOAD + 1, "https://github.com/update.dmg"),
                3
            )
            .is_err()
        );
        assert!(release(&feed(4, 20, "http://example.com/update.dmg"), 3).is_err());
        assert!(release(&feed(4, 20, "file:///tmp/update.dmg"), 3).is_err());
        assert!(release(&feed(4, 20, "http://127.0.0.1/update.dmg"), 3).is_ok());
    }

    #[test]
    fn bundle_exchange_is_atomic_and_reversible() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Minterm.app");
        let replacement = dir.path().join("new.app");
        fs::create_dir(&target).unwrap();
        fs::create_dir(&replacement).unwrap();
        fs::write(target.join("version"), "old").unwrap();
        fs::write(replacement.join("version"), "new").unwrap();
        exchange(&target, &replacement).unwrap();
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(replacement.join("version")).unwrap(),
            "old"
        );
        exchange(&target, &replacement).unwrap();
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "old");
        assert!(exchange(&target, &dir.path().join("missing")).is_err());
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "old");
    }
}

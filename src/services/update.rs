//! Self-update from GitHub releases. The only requests are to api.github.com and github.com, and nothing is installed
//! until the user asks for it.
//!
//! Two independent checks guard the download, because Camel runs as administrator and installs what it downloads:
//! - SHA-256 from the release: catches a broken or truncated download.
//! - Ed25519 signature (`camel.exe.sig`) against `SIGNING_KEY`, compiled in: proves the exe was built by the release
//!   workflow, which alone holds the private key (GitHub secret `UPDATE_SIGNING_KEY`). Someone who can upload release
//!   assets, but has not got the key, cannot ship an update. A release without a valid signature is never installed.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "aw4e/Camel";
const ASSET: &str = "camel.exe";
const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;
/// Public half of the release signing key (raw 32-byte Ed25519). Rotating the key: put the new public key here, ship
/// that build signed with the OLD key, and only then switch the workflow secret to the new private key.
const SIGNING_KEY: [u8; 32] = [
    0xf6, 0x74, 0x07, 0x6f, 0x17, 0xbe, 0xcd, 0x91, 0xc9, 0xf7, 0x78, 0xd5, 0x35, 0x9f, 0x2e, 0xd4, 0xd1, 0x94, 0x8c, 0x5a, 0xa3, 0x97, 0x75, 0x06, 0x14, 0x72,
    0x5f, 0x1b, 0xd4, 0x90, 0xba, 0x35,
];

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    url: String,
    sha256: String,
    sig_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing,
    Failed(String),
}

#[derive(Deserialize)]
struct Api {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// "v1.2.3" or "1.2.3" as numbers. Pre-release suffixes ("1.2.3-beta") are not versions we ship.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.trim().trim_start_matches('v').split('.');
    let v = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    matches!((parse_version(current), parse_version(latest)), (Some(c), Some(l)) if l > c)
}

fn download_prefix() -> String {
    format!("https://github.com/{REPO}/releases/download/")
}

fn valid_sha256(s: &str) -> Option<String> {
    let hex = s.trim().strip_prefix("sha256:").unwrap_or(s.trim()).to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// The newer release in an API answer. The asset must live under this repository's release downloads, have a
/// `camel.exe.sig` beside it, and carry a SHA-256, either in the API's `digest` field or in a `camel.exe.sha256` asset
/// the caller fetches (`checksum_url`).
fn pick(json: &str, current: &str) -> Result<Option<(Release, Option<String>)>, String> {
    let api: Api = serde_json::from_str(json).map_err(|e| format!("unreadable answer: {e}"))?;
    if !is_newer(current, &api.tag_name) {
        return Ok(None);
    }
    let exe = api.assets.iter().find(|a| a.name == ASSET).ok_or("release has no camel.exe")?;
    if !exe.browser_download_url.starts_with(&download_prefix()) {
        return Err("download is not hosted by the repository".into());
    }
    let asset_url = |name: String| {
        api.assets.iter().find(|a| a.name == name && a.browser_download_url.starts_with(&download_prefix())).map(|a| a.browser_download_url.clone())
    };
    let checksum_url = asset_url(format!("{ASSET}.sha256"));
    let sig_url = asset_url(format!("{ASSET}.sig")).ok_or("release is not signed")?;
    let sha256 = exe.digest.as_deref().and_then(valid_sha256).unwrap_or_default();
    if sha256.is_empty() && checksum_url.is_none() {
        return Err("release has no checksum".into());
    }
    let version = api.tag_name.trim_start_matches('v').to_owned();
    Ok(Some((Release { version, url: exe.browser_download_url.clone(), sha256, sig_url }, checksum_url)))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into()
}

fn check_now() -> Result<Option<Release>, String> {
    let agent = agent();
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = agent
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "camel-updater")
        .call()
        .map_err(|e| format!("could not reach GitHub: {e}"))?;
    let json = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let Some((mut release, checksum_url)) = pick(&json, VERSION)? else { return Ok(None) };
    if release.sha256.is_empty()
        && let Some(url) = checksum_url
    {
        let text =
            agent.get(&url).header("User-Agent", "camel-updater").call().map_err(|e| e.to_string())?.body_mut().read_to_string().map_err(|e| e.to_string())?;
        release.sha256 = text.split_whitespace().next().and_then(valid_sha256).ok_or("checksum file is not a SHA-256")?;
    }
    Ok(Some(release))
}

fn sha256_hex(mut r: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// True when `file` is a Windows executable whose SHA-256 is `expected`.
fn verify(file: &Path, expected: &str) -> bool {
    let mut magic = [0u8; 2];
    let Ok(mut f) = std::fs::File::open(file) else { return false };
    if f.read_exact(&mut magic).is_err() || &magic != b"MZ" {
        return false;
    }
    std::fs::File::open(file).and_then(sha256_hex).is_ok_and(|h| h == expected)
}

/// True when `sig` is a valid Ed25519 signature by `key` over the whole of `file`.
fn verify_signature(key: &[u8; 32], file: &Path, sig: &[u8]) -> bool {
    use ring::signature::{ED25519, UnparsedPublicKey};
    std::fs::read(file).is_ok_and(|bytes| UnparsedPublicKey::new(&ED25519, key).verify(&bytes, sig).is_ok())
}

fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

fn download(release: &Release, to: &Path) -> Result<(), String> {
    if !release.url.starts_with(&download_prefix()) {
        return Err("download is not hosted by the repository".into());
    }
    let mut resp = agent().get(&release.url).header("User-Agent", "camel-updater").call().map_err(|e| format!("download failed: {e}"))?;
    let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    // remove, then create_new: never write through a file or link someone else left at this path
    let _ = std::fs::remove_file(to);
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(to).map_err(|e| format!("cannot write next to the program: {e}"))?;
    std::io::copy(&mut reader, &mut out).map_err(|e| format!("download failed: {e}"))?;
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if !verify(to, &release.sha256) {
        let _ = std::fs::remove_file(to);
        return Err("download does not match its checksum".into());
    }
    let sig = fetch_signature(&release.sig_url)?;
    if !verify_signature(&SIGNING_KEY, to, &sig) {
        let _ = std::fs::remove_file(to);
        return Err("download is not signed by the Camel release key".into());
    }
    Ok(())
}

/// The 64-byte signature asset; anything larger is refused unread.
fn fetch_signature(url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with(&download_prefix()) {
        return Err("signature is not hosted by the repository".into());
    }
    let mut resp = agent().get(url).header("User-Agent", "camel-updater").call().map_err(|e| format!("signature download failed: {e}"))?;
    let mut sig = Vec::new();
    resp.body_mut().with_config().limit(1024).reader().read_to_end(&mut sig).map_err(|e| format!("signature download failed: {e}"))?;
    if sig.len() != 64 {
        return Err("signature has the wrong size".into());
    }
    Ok(sig)
}

/// Puts `staged` in place of `exe`. A running exe can be renamed but not overwritten, so the old one is moved aside
/// first and put back if the new one cannot take its place.
fn swap(exe: &Path, staged: &Path) -> std::io::Result<()> {
    let old = sibling(exe, ".old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old)?;
    if let Err(e) = std::fs::rename(staged, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(e);
    }
    Ok(())
}

/// Removes what a finished update left behind. Called once at startup.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(sibling(&exe, ".old"));
        let _ = std::fs::remove_file(sibling(&exe, ".new"));
    }
}

/// Restarts into the installed copy. It keeps the administrator rights this one has, so no second prompt appears.
fn relaunch(exe: &Path) -> bool {
    use windows::Win32::UI::Shell::IsUserAnAdmin;
    let arg = if unsafe { IsUserAnAdmin() }.as_bool() { "--elevated" } else { "--no-elevate" };
    std::process::Command::new(exe).arg(arg).spawn().is_ok()
}

#[derive(Clone)]
pub struct Updater {
    status: Arc<Mutex<Status>>,
    restart: Arc<AtomicBool>,
}

impl Updater {
    pub fn new() -> Self {
        Self { status: Arc::new(Mutex::new(Status::Idle)), restart: Arc::new(AtomicBool::new(false)) }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn set(&self, s: Status) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = s;
    }

    /// True once the new copy runs and this one should close.
    pub fn restart_due(&self) -> bool {
        self.restart.load(Ordering::Relaxed)
    }

    /// Looks for a newer release in the background. Does nothing while a check or an install is running.
    pub fn check(&self) {
        if matches!(self.status(), Status::Checking | Status::Installing) {
            return;
        }
        self.set(Status::Checking);
        let me = self.clone();
        std::thread::spawn(move || {
            me.set(match check_now() {
                Ok(Some(r)) => Status::Available(r),
                Ok(None) => Status::UpToDate,
                Err(e) => Status::Failed(e),
            });
        });
    }

    /// Downloads, verifies and swaps in the available release, then starts it.
    pub fn install(&self) {
        let Status::Available(release) = self.status() else { return };
        self.set(Status::Installing);
        let me = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let staged = sibling(&exe, ".new");
                download(&release, &staged)?;
                swap(&exe, &staged).map_err(|e| format!("cannot replace the program: {e}"))?;
                // someone with write access to the folder could have replaced the file between the check and the swap
                if !verify(&exe, &release.sha256) {
                    let _ = std::fs::rename(sibling(&exe, ".old"), &exe);
                    return Err("the installed file does not match its checksum, kept the old version".to_owned());
                }
                if relaunch(&exe) { Ok(()) } else { Err("updated, restart Camel to use it".to_owned()) }
            })();
            match result {
                Ok(()) => me.restart.store(true, Ordering::Relaxed),
                Err(e) => me.set(Status::Failed(e)),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(tag: &str, url: &str, digest: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","assets":[{{"name":"camel.exe","browser_download_url":"{url}","digest":{digest}}},{{"name":"camel.exe.sig","browser_download_url":"{GOOD_URL}.sig"}}]}}"#
        )
    }

    const GOOD_URL: &str = "https://github.com/aw4e/Camel/releases/download/v9.0.0/camel.exe";
    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.10.0"), Some((1, 10, 0)));
        for bad in ["", "1.2", "1.2.3.4", "1.2.x", "1.2.3-beta", "latest"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert!(is_newer("0.2.0", "v0.10.0"), "0.10 is newer than 0.2");
        assert!(is_newer("0.2.0", "1.0.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2.1", "0.2.0"));
        assert!(!is_newer("0.2.0", "garbage"), "an unreadable tag never triggers an update");
    }

    #[test]
    fn this_build_has_a_real_version() {
        assert!(parse_version(VERSION).is_some(), "{VERSION}");
    }

    #[test]
    fn picks_a_newer_release_with_its_checksum() {
        let json = api("v9.0.0", GOOD_URL, &format!("\"sha256:{HEX}\""));
        let (r, checksum_url) = pick(&json, "0.2.0").unwrap().unwrap();
        assert_eq!((r.version.as_str(), r.sha256.as_str(), checksum_url), ("9.0.0", HEX, None));
        assert!(pick(&json, "9.0.0").unwrap().is_none(), "already up to date");
    }

    #[test]
    fn refuses_releases_it_cannot_trust() {
        let foreign = api("v9.0.0", "https://evil.example/camel.exe", &format!("\"sha256:{HEX}\""));
        assert!(pick(&foreign, "0.2.0").is_err(), "download from another host");
        let lookalike = api("v9.0.0", "https://github.com/aw4e/Camel-evil/releases/download/v9/camel.exe", &format!("\"sha256:{HEX}\""));
        assert!(pick(&lookalike, "0.2.0").is_err(), "download from another repository");
        assert!(pick(&api("v9.0.0", GOOD_URL, "null"), "0.2.0").is_err(), "no checksum anywhere");
        assert!(pick(&api("v9.0.0", GOOD_URL, "\"sha256:xyz\""), "0.2.0").is_err(), "checksum is not hex");
        assert!(pick("{}", "0.2.0").is_err());
        assert!(pick(r#"{"tag_name":"v9.0.0","assets":[]}"#, "0.2.0").is_err(), "no exe asset");
        let unsigned = format!(r#"{{"tag_name":"v9.0.0","assets":[{{"name":"camel.exe","browser_download_url":"{GOOD_URL}","digest":"sha256:{HEX}"}}]}}"#);
        assert_eq!(pick(&unsigned, "0.2.0").unwrap_err(), "release is not signed");
    }

    #[test]
    fn a_checksum_asset_stands_in_for_the_digest_field() {
        let json = format!(
            r#"{{"tag_name":"v9.0.0","assets":[{{"name":"camel.exe","browser_download_url":"{GOOD_URL}"}},{{"name":"camel.exe.sha256","browser_download_url":"{GOOD_URL}.sha256"}},{{"name":"camel.exe.sig","browser_download_url":"{GOOD_URL}.sig"}}]}}"#
        );
        let (r, checksum_url) = pick(&json, "0.2.0").unwrap().unwrap();
        assert!(r.sha256.is_empty());
        assert_eq!(checksum_url.as_deref(), Some(format!("{GOOD_URL}.sha256").as_str()));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("camel-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hashes_and_verifies_files() {
        assert_eq!(sha256_hex(&b"abc"[..]).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let dir = scratch("verify");
        let file = dir.join("camel.exe.new");
        std::fs::write(&file, b"MZ fake program").unwrap();
        let good = sha256_hex(&b"MZ fake program"[..]).unwrap();
        assert!(verify(&file, &good));
        assert!(!verify(&file, HEX), "wrong checksum");
        std::fs::write(&file, b"not an exe").unwrap();
        assert!(!verify(&file, &sha256_hex(&b"not an exe"[..]).unwrap()), "right checksum, but not an executable");
        assert!(!verify(&dir.join("missing"), HEX));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_compiled_in_key_matches_the_release_signing_key() {
        // made with: openssl pkeyutl -sign -inkey <release key> -rawin -in <file holding the message>
        const SIG: [u8; 64] = [
            0x8a, 0x37, 0xfb, 0xb1, 0x58, 0x0f, 0x78, 0x8f, 0xbf, 0xda, 0x7a, 0x08, 0xf0, 0xdb, 0x75, 0xd9, 0x28, 0x0c, 0x23, 0x67, 0xed, 0xda, 0x80, 0xd2,
            0x0b, 0x27, 0xec, 0x76, 0xaf, 0x01, 0x08, 0xc1, 0xc0, 0xdf, 0x9a, 0xc6, 0xab, 0x6d, 0xfc, 0x3c, 0xa8, 0x4a, 0x88, 0xeb, 0xc5, 0xf1, 0x8a, 0x92,
            0x71, 0x43, 0x01, 0xc8, 0xb8, 0x5a, 0x10, 0x3a, 0xe1, 0x7f, 0x9f, 0xb5, 0xdd, 0xfd, 0x97, 0x0d,
        ];
        let dir = scratch("key");
        let file = dir.join("msg");
        std::fs::write(&file, b"MZ camel signing self-check").unwrap();
        assert!(verify_signature(&SIGNING_KEY, &file, &SIG), "SIGNING_KEY does not belong to the key the workflow signs with");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_a_signature_by_the_key_over_the_exact_file_passes() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        let dir = scratch("sig");
        let file = dir.join("camel.exe.new");
        std::fs::write(&file, b"MZ signed program").unwrap();
        let sig = pair.sign(b"MZ signed program");
        assert!(verify_signature(&key, &file, sig.as_ref()));
        assert!(!verify_signature(&SIGNING_KEY, &file, sig.as_ref()), "signed by another key");
        std::fs::write(&file, b"MZ signed programX").unwrap();
        assert!(!verify_signature(&key, &file, sig.as_ref()), "file changed after signing");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn swapping_keeps_the_old_copy_aside_and_rolls_back() {
        let dir = scratch("swap");
        let (exe, staged) = (dir.join("camel.exe"), dir.join("camel.exe.new"));
        std::fs::write(&exe, b"old").unwrap();
        std::fs::write(&staged, b"new").unwrap();
        swap(&exe, &staged).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(sibling(&exe, ".old")).unwrap(), b"old");
        assert!(!staged.exists());
        // the staged file is gone, so a second swap must fail and leave the installed copy untouched
        assert!(swap(&exe, &staged).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(dir);
    }
}

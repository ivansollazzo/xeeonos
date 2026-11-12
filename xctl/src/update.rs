//! `xctl update`: download, verify and install a new XeeonOS version.
//!
//! An update is a directory of files (see tools/mkupdate.sh in the repo):
//!
//!   xeeonos_<v>.efi                              kernel (UKI)
//!   xeeonos_<v>.usr-x86-64.<uuid>.raw.zst         /usr partition
//!   xeeonos_<v>.usr-x86-64-verity.<uuid>.raw.zst  its dm-verity hash tree
//!   SHA256SUMS, SHA256SUMS.gpg                    manifest + signature
//!
//! Steps: fetch the signed manifest (a few hundred bytes), check its
//! signature against the keys shipped in /usr, and read the version from the
//! kernel's name in it. Only when that version is newer than the running one
//! (and did not already fail here) fetch the files it lists, check their
//! hashes, then let systemd-sysupdate write them into the free /usr slot and
//! the ESP. The running system is never modified: the new version starts at
//! the next boot. `--check` stops after the version check, without root.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::boot;
use crate::channel;
use crate::config;
use crate::oci;
use crate::util::{Result, err, need_root, output, run as run_program};
use crate::version::{compare, read_os_release};

/// Where updates come from.
#[derive(Debug, PartialEq)]
enum Source {
    /// An OCI artifact on a registry, e.g. ghcr.io/ivansollazzo/xeeonos:experimental
    Oci(String),
    /// A web directory, e.g. http://10.0.2.2:8080
    Http(String),
    /// A local directory, e.g. a USB stick.
    Dir(PathBuf),
}

impl Source {
    fn parse(s: &str) -> Source {
        if let Some(reference) = s.strip_prefix("oci://") {
            Source::Oci(reference.to_string())
        } else if s.starts_with("http://") || s.starts_with("https://") {
            Source::Http(s.trim_end_matches('/').to_string())
        } else {
            Source::Dir(PathBuf::from(s))
        }
    }
}

pub fn run(args: &[String]) -> Result<()> {
    // Parse the options.
    let mut from = None;
    let mut check = false;
    let mut keyrings = vec![PathBuf::from(config::KEYRING)];
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--from" => {
                from = Some(
                    it.next()
                        .ok_or_else(|| err("--from needs a source"))?
                        .clone(),
                )
            }
            "--check" => check = true,
            "--keyring" => keyrings.push(PathBuf::from(
                it.next().ok_or_else(|| err("--keyring needs a file"))?,
            )),
            other => return Err(err(format!("unknown option for update: {other}"))),
        }
    }
    if !check {
        need_root("update")?;
    }

    let os = read_os_release(config::OS_RELEASE)?;
    let current = os.get("IMAGE_VERSION").cloned().unwrap_or_default();

    // Without --from, use the channel's tag on the registry.
    let source = match from {
        Some(f) => Source::parse(&f),
        None => {
            let Some(channel) = channel::current()? else {
                if check {
                    // The daily check on a development build: nothing to follow.
                    println!(
                        "This is a development build without an update channel: nothing to check."
                    );
                    return Ok(());
                }
                return Err(err(
                    "this is a development build without an update channel; use --from <source>",
                ));
            };
            Source::Oci(format!("{}:{channel}", config::REGISTRY))
        }
    };

    // Start from an empty download directory, and clean it up at the end
    // whatever happens (the `Cleanup` value removes it when dropped).
    // (--check runs without root: it uses a private temporary directory.)
    let check_dir = std::env::temp_dir().join(format!("xctl-check-{}", std::process::id()));
    let cache = if check {
        check_dir.as_path()
    } else {
        Path::new(config::CACHE)
    };
    let _ = fs::remove_dir_all(cache);
    fs::create_dir_all(cache)?;
    let _cleanup = Cleanup(cache);

    // First only the signed manifest: it says which version the source has.
    fetch_manifest(&source, cache, &keyrings)?;
    let names = manifest_files(&fs::read_to_string(cache.join("SHA256SUMS"))?)?;
    let new = versions_in(&names)
        .into_iter()
        .max_by(|a, b| compare(a, b))
        .ok_or_else(|| err("the source contains no XeeonOS version"))?;
    // Whatever the outcome, the "update available" note is refreshed below.
    set_available(None);
    match compare(&new, &current) {
        Ordering::Equal => {
            println!("XeeonOS {current} is up to date.");
            return Ok(());
        }
        Ordering::Less => {
            println!(
                "The source has XeeonOS {new}, older than the running {current}: nothing to do."
            );
            return Ok(());
        }
        Ordering::Greater => {}
    }
    // A version that already failed to start here (or was rolled back) is
    // not installed again: wait for a newer one.
    if boot::bad_versions().contains(&new) {
        println!(
            "XeeonOS {new} did not work on this computer and was disabled: waiting for a newer version."
        );
        return Ok(());
    }
    if check {
        set_available(Some(&new));
        println!(
            "XeeonOS {new} is available (running: {current}). Install it with: sudo xctl update"
        );
        return Ok(());
    }

    // Only now the large files.
    fetch_files(&source, cache, &names)?;
    check_hashes(cache)?;
    // A copy of the data to go back to (xctl snapshots), if it can be taken.
    match crate::snapshots::create(&format!("before-{new}")) {
        Ok(name) => println!("Snapshot of the data: {name}"),
        Err(e) => println!("No snapshot of the data: {e}"),
    }
    println!("Installing XeeonOS {new} (running: {current})...");
    // The files were verified above against the signed manifest.
    run_program(config::SYSUPDATE, &["update", &new])?;
    println!("XeeonOS {new} is installed. Restart to start using it.");
    Ok(())
}

/// Remember (or forget) that a newer version is available, for the message
/// at login and `xctl status`. Only root can write it (the daily check runs as
/// root); for others this does nothing.
fn set_available(version: Option<&str>) {
    let file = Path::new(config::UPDATE_AVAILABLE);
    match version {
        Some(v) => {
            if let Some(dir) = file.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(file, format!("{v}\n"));
        }
        None => {
            let _ = fs::remove_file(file);
        }
    }
}

/// The version noted by the last check, if a newer one is available.
pub fn available() -> Option<String> {
    let v = fs::read_to_string(config::UPDATE_AVAILABLE).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Removes a directory when it goes out of scope (Rust's "Drop" trait).
struct Cleanup<'a>(&'a Path);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}

/// Put the signed manifest (SHA256SUMS) into `dir` and verify its signature.
fn fetch_manifest(source: &Source, dir: &Path, keyrings: &[PathBuf]) -> Result<()> {
    if !matches!(source, Source::Dir(_)) {
        wait_for_network();
    }
    match source {
        Source::Oci(reference) => {
            println!("Checking {reference}...");
            let artifact = oci::Artifact::open(reference)?;
            artifact.download("SHA256SUMS", &dir.join("SHA256SUMS"))?;
            // A missing signature is reported by verify_signature.
            let _ = artifact.download("SHA256SUMS.gpg", &dir.join("SHA256SUMS.gpg"));
        }
        Source::Http(base) => {
            println!("Checking {base}...");
            download(&format!("{base}/SHA256SUMS"), &dir.join("SHA256SUMS"))?;
            // A missing signature is reported by verify_signature.
            let _ = download(
                &format!("{base}/SHA256SUMS.gpg"),
                &dir.join("SHA256SUMS.gpg"),
            );
        }
        Source::Dir(from) => {
            if !from.join("SHA256SUMS").exists() {
                return Err(err(format!("{} has no SHA256SUMS", from.display())));
            }
            fs::copy(from.join("SHA256SUMS"), dir.join("SHA256SUMS"))?;
            if from.join("SHA256SUMS.gpg").exists() {
                fs::copy(from.join("SHA256SUMS.gpg"), dir.join("SHA256SUMS.gpg"))?;
            }
        }
    }
    verify_signature(dir, keyrings)
}

/// Put the files listed in the (verified) manifest into `dir`.
fn fetch_files(source: &Source, dir: &Path, names: &[String]) -> Result<()> {
    match source {
        Source::Oci(reference) => {
            println!("Downloading {reference}...");
            let artifact = oci::Artifact::open(reference)?;
            for name in names {
                artifact.download(name, &dir.join(name))?;
            }
        }
        Source::Http(base) => {
            println!("Downloading from {base}...");
            for name in names {
                download(&format!("{base}/{name}"), &dir.join(name))?;
            }
        }
        Source::Dir(from) => {
            println!("Copying from {}...", from.display());
            for name in names {
                fs::copy(from.join(name), dir.join(name))?;
            }
        }
    }
    Ok(())
}

/// Right after boot the network may still be coming up (NetworkManager
/// itself may not be running yet): wait up to a minute for a connection. If
/// it never comes, go on and let the download report the error.
fn wait_for_network() {
    for _ in 0..30 {
        let online = std::process::Command::new("nm-online")
            .args(["--quiet", "--timeout=2"])
            .status()
            .is_ok_and(|s| s.success());
        if online {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// Download `url` into the file `to` with curl, retrying on network errors.
/// `headers` are extra HTTP headers, e.g. "Authorization: Bearer ...".
pub(crate) fn download_with(url: &str, to: &Path, headers: &[String]) -> Result<()> {
    let to = to.to_str().ok_or_else(|| err("non-UTF-8 path"))?;
    let mut args: Vec<&str> = vec![
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        // Retry transient failures (timeouts, 5xx, refused connections),
        // but not answers like 404.
        "--retry",
        "5",
        "--retry-connrefused",
        "--retry-delay",
        "3",
    ];
    for h in headers {
        args.push("--header");
        args.push(h);
    }
    args.extend(["--output", to, url]);
    output("curl", &args)?;
    Ok(())
}

fn download(url: &str, to: &Path) -> Result<()> {
    download_with(url, to, &[])
}

/// The file names listed in a SHA256SUMS manifest ("<hash>  <name>" lines).
/// Anything that is not a plain file name is refused, so a manifest can never
/// make us write outside the download directory.
/// A plain file name: letters, digits and `._+~-`, not starting with a dot.
/// No "/" means it can only ever name a file inside the download directory.
pub(crate) fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+~-".contains(c))
}

pub(crate) fn manifest_files(manifest: &str) -> Result<Vec<String>> {
    manifest
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let name = line
                .split_whitespace()
                .nth(1)
                .ok_or_else(|| err(format!("malformed SHA256SUMS line: {line}")))?;
            let name = name.trim_start_matches('*');
            if is_plain_file_name(name) {
                Ok(name.to_string())
            } else {
                Err(err(format!("unexpected file name in SHA256SUMS: {name}")))
            }
        })
        .collect()
}

/// Check SHA256SUMS.gpg against the trusted keys with gpgv.
fn verify_signature(dir: &Path, keyrings: &[PathBuf]) -> Result<()> {
    let existing: Vec<&PathBuf> = keyrings.iter().filter(|k| k.exists()).collect();
    if existing.is_empty() {
        return Err(err(format!(
            "no update signing key on this system ({}); use --keyring <file>",
            config::KEYRING
        )));
    }
    if !dir.join("SHA256SUMS.gpg").exists() {
        return Err(err("the update is not signed (SHA256SUMS.gpg missing)"));
    }

    // gpgv wants a home directory; give it a throwaway one.
    let home = std::env::temp_dir().join(format!("xctl-gpg-{}", std::process::id()));
    fs::create_dir_all(&home)?;
    let _cleanup = Cleanup(&home);

    let mut cmd = Command::new("gpgv");
    cmd.env("GNUPGHOME", &home);
    for k in existing {
        // gpgv treats names without a "/" as relative to its home directory.
        cmd.arg("--keyring").arg(fs::canonicalize(k)?);
    }
    cmd.arg(dir.join("SHA256SUMS.gpg"))
        .arg(dir.join("SHA256SUMS"));
    let out = cmd
        .output()
        .map_err(|e| err(format!("cannot run gpgv: {e}")))?;
    if !out.status.success() {
        eprint!("{}", String::from_utf8_lossy(&out.stderr));
        return Err(err(
            "the update signature is not valid; nothing was installed",
        ));
    }
    println!("Signature: valid");
    Ok(())
}

/// Check every file against the (already verified) manifest.
pub(crate) fn check_hashes(dir: &Path) -> Result<()> {
    let ok = Command::new("sha256sum")
        .args(["--check", "--strict", "--quiet", "SHA256SUMS"])
        .current_dir(dir)
        .status()
        .map_err(|e| err(format!("cannot run sha256sum: {e}")))?
        .success();
    if ok {
        Ok(())
    } else {
        Err(err(
            "downloaded files do not match the signed manifest; nothing was installed",
        ))
    }
}

/// The newest version among the kernels (xeeonos_<v>.efi) in `dir`.
pub(crate) fn newest_version(dir: &Path) -> Result<Option<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(versions_in(&names).into_iter().max_by(|a, b| compare(a, b)))
}

fn versions_in(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter_map(|n| n.strip_prefix("xeeonos_")?.strip_suffix(".efi"))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources() {
        assert_eq!(
            Source::parse("oci://ghcr.io/a/b:tag"),
            Source::Oci("ghcr.io/a/b:tag".into())
        );
        assert_eq!(
            Source::parse("http://10.0.2.2:8080/"),
            Source::Http("http://10.0.2.2:8080".into())
        );
        assert_eq!(
            Source::parse("/run/media/usb"),
            Source::Dir("/run/media/usb".into())
        );
    }

    #[test]
    fn manifest_names() {
        let m = "abc  xeeonos_0.2.efi\ndef *xeeonos_0.2.usr-x86-64.1234.raw.zst\n\n";
        assert_eq!(
            manifest_files(m).unwrap(),
            vec!["xeeonos_0.2.efi", "xeeonos_0.2.usr-x86-64.1234.raw.zst"]
        );
        assert!(manifest_files("abc  ../etc/passwd\n").is_err());
        assert!(manifest_files("abc  /etc/passwd\n").is_err());
        assert!(manifest_files("abc  .hidden\n").is_err());
        assert!(manifest_files("abc\n").is_err());
    }

    #[test]
    fn newest() {
        let names: Vec<String> = [
            "xeeonos_0.1.0-experimental.9.efi",
            "xeeonos_0.1.0-experimental.12.efi",
            "SHA256SUMS",
        ]
        .map(String::from)
        .to_vec();
        let newest = versions_in(&names).into_iter().max_by(|a, b| compare(a, b));
        assert_eq!(newest.as_deref(), Some("0.1.0-experimental.12"));
    }
}

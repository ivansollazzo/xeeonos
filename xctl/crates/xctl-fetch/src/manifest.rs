//! Getting XeeonOS files: from a registry (oci), a web directory or a
//! local directory, checked against a signed manifest (SHA256SUMS +
//! SHA256SUMS.gpg). Used by updates, extensions and the installer.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::oci;
use crate::progress;
use xctl_core::config;
use xctl_core::util::{Result, err, output};
use xctl_core::version::compare;

/// Where updates come from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// An OCI artifact on a registry, e.g. ghcr.io/ivansollazzo/xeeonos:experimental
    Oci(String),
    /// A web directory, e.g. http://10.0.2.2:8080
    Http(String),
    /// A local directory, e.g. a USB stick.
    Dir(PathBuf),
}

impl Source {
    pub fn parse(s: &str) -> Source {
        if let Some(reference) = s.strip_prefix("oci://") {
            Source::Oci(reference.to_string())
        } else if s.starts_with("http://") || s.starts_with("https://") {
            Source::Http(s.trim_end_matches('/').to_string())
        } else {
            Source::Dir(PathBuf::from(s))
        }
    }
}

/// Removes a directory when it goes out of scope (Rust's "Drop" trait).
pub struct Cleanup<'a>(pub &'a Path);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}

/// Put the signed manifest (SHA256SUMS) into `dir` and verify its signature.
pub fn fetch_manifest(source: &Source, dir: &Path, keyrings: &[PathBuf]) -> Result<()> {
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
pub fn fetch_files(source: &Source, dir: &Path, names: &[String]) -> Result<()> {
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
                let url = format!("{base}/{name}");
                let size = if progress::watching() {
                    remote_size(&url)
                } else {
                    None
                };
                download_large(&url, &dir.join(name), &[], size)?;
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
pub fn download_with(url: &str, to: &Path, headers: &[String]) -> Result<()> {
    download_sized(url, to, headers, None)
}

/// Like `download_with`, for a file worth a progress line (shown only when
/// someone is watching, see `progress`): `total` is its size, if known.
pub fn download_large(url: &str, to: &Path, headers: &[String], total: Option<u64>) -> Result<()> {
    if !progress::watching() {
        return download_with(url, to, headers);
    }
    download_sized(url, to, headers, Some(total))
}

/// curl, with a progress line when `progress` is Some(size if known).
fn download_sized(
    url: &str,
    to: &Path,
    headers: &[String],
    progress: Option<Option<u64>>,
) -> Result<()> {
    let to_path = to;
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
    if let Some(total) = progress {
        let label = to_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return progress::curl_with_progress(&args, url, to_path, &label, total);
    }
    args.extend(["--output", to, url]);
    output("curl", &args)?;
    Ok(())
}

/// The size of the file at `url` (HTTP Content-Length), if the server says.
fn remote_size(url: &str) -> Option<u64> {
    let head = output("curl", &["--fail", "--silent", "--head", "--location", url]).ok()?;
    head.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<u64>().ok())?
        })
        .next_back()
}

fn download(url: &str, to: &Path) -> Result<()> {
    download_with(url, to, &[])
}

/// The file names listed in a SHA256SUMS manifest ("<hash>  <name>" lines).
/// Anything that is not a plain file name is refused, so a manifest can never
/// make us write outside the download directory.
/// A plain file name: letters, digits and `._+~-`, not starting with a dot.
/// No "/" means it can only ever name a file inside the download directory.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+~-".contains(c))
}

pub fn manifest_files(manifest: &str) -> Result<Vec<String>> {
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

/// The SHA-256 of each file in a (verified) SHA256SUMS manifest, by name.
pub fn manifest_hashes(manifest: &str) -> Vec<(String, String)> {
    manifest
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let hash = words.next()?;
            let name = words.next()?.trim_start_matches('*');
            (hash.len() == 64 && is_plain_file_name(name))
                .then(|| (name.to_string(), hash.to_lowercase()))
        })
        .collect()
}

/// The SHA-256 of a file on this computer.
pub fn file_hash(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or_else(|| err("non-UTF-8 path"))?;
    let out = output("sha256sum", &["--", path])?;
    out.split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| err(format!("sha256sum gave nothing for {path}")))
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
pub fn check_hashes(dir: &Path) -> Result<()> {
    let ok = Command::new("sha256sum")
        // --ignore-missing: the manifest also lists the optional extensions
        // (xctl extensions), which are downloaded only where they are turned on.
        .args([
            "--check",
            "--strict",
            "--quiet",
            "--ignore-missing",
            "SHA256SUMS",
        ])
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
pub fn newest_version(dir: &Path) -> Result<Option<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(versions_in(&names).into_iter().max_by(|a, b| compare(a, b)))
}

pub fn versions_in(names: &[String]) -> Vec<String> {
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
    fn hashes() {
        let h = "a".repeat(64);
        let m = format!("{h}  xeeonos_1.docker.xext\n{h} *docker.conf\nshort  x\n");
        assert_eq!(
            manifest_hashes(&m),
            vec![
                ("xeeonos_1.docker.xext".to_string(), h.clone()),
                ("docker.conf".to_string(), h.clone())
            ]
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

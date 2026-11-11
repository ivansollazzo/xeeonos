//! `xctl update`: download, verify and install a new XeeonOS version.
//!
//! An update is a directory of files (see tools/mkupdate.sh in the repo):
//!
//!   xeeonos_<v>.efi                              kernel (UKI)
//!   xeeonos_<v>.usr-x86-64.<uuid>.raw.zst         /usr partition
//!   xeeonos_<v>.usr-x86-64-verity.<uuid>.raw.zst  its dm-verity hash tree
//!   SHA256SUMS, SHA256SUMS.gpg                    manifest + signature
//!
//! Steps: fetch the signed manifest, check its signature against the keys
//! shipped in /usr, fetch the files it lists, check their hashes, then let
//! systemd-sysupdate write them into the free /usr slot and the ESP. The
//! running system is never modified: the new version starts at the next boot.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::channel;
use crate::config;
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
            "--keyring" => keyrings.push(PathBuf::from(
                it.next().ok_or_else(|| err("--keyring needs a file"))?,
            )),
            other => return Err(err(format!("unknown option for update: {other}"))),
        }
    }
    need_root("update")?;

    let os = read_os_release(config::OS_RELEASE)?;
    let current = os.get("IMAGE_VERSION").cloned().unwrap_or_default();

    // Without --from, use the channel's tag on the registry.
    let source = match from {
        Some(f) => Source::parse(&f),
        None => {
            let channel = channel::current()?.ok_or_else(|| {
                err("this is a development build without an update channel; use --from <source>")
            })?;
            Source::Oci(format!("{}:{channel}", config::REGISTRY))
        }
    };

    // Start from an empty download directory, and clean it up at the end
    // whatever happens (the `Cleanup` value removes it when dropped).
    let cache = Path::new(config::CACHE);
    let _ = fs::remove_dir_all(cache);
    fs::create_dir_all(cache)?;
    let _cleanup = Cleanup(cache);

    fetch(&source, cache, &keyrings)?;

    let new =
        newest_version(cache)?.ok_or_else(|| err("the source contains no XeeonOS version"))?;
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

    println!("Installing XeeonOS {new} (running: {current})...");
    // The files were verified above against the signed manifest.
    run_program(config::SYSUPDATE, &["update", &new])?;
    println!("XeeonOS {new} is installed. Restart to start using it.");
    Ok(())
}

/// Removes a directory when it goes out of scope (Rust's "Drop" trait).
struct Cleanup<'a>(&'a Path);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}

/// Put the update files into `dir`, verifying the signed manifest first.
fn fetch(source: &Source, dir: &Path, keyrings: &[PathBuf]) -> Result<()> {
    let dir_str = dir.to_str().ok_or_else(|| err("non-UTF-8 path"))?;
    match source {
        Source::Oci(reference) => {
            println!("Downloading {reference}...");
            // oras downloads every file of the artifact; we check them all below.
            output(
                "oras",
                &["pull", "--no-tty", "--output", dir_str, reference],
            )?;
            verify_signature(dir, keyrings)?;
        }
        Source::Http(base) => {
            println!("Downloading from {base}...");
            download(&format!("{base}/SHA256SUMS"), &dir.join("SHA256SUMS"))?;
            // A missing signature is reported by verify_signature.
            let _ = download(
                &format!("{base}/SHA256SUMS.gpg"),
                &dir.join("SHA256SUMS.gpg"),
            );
            verify_signature(dir, keyrings)?;
            for name in manifest_files(&fs::read_to_string(dir.join("SHA256SUMS"))?)? {
                download(&format!("{base}/{name}"), &dir.join(&name))?;
            }
        }
        Source::Dir(from) => {
            if !from.join("SHA256SUMS").exists() {
                return Err(err(format!("{} has no SHA256SUMS", from.display())));
            }
            fs::copy(from.join("SHA256SUMS"), dir.join("SHA256SUMS"))?;
            if from.join("SHA256SUMS.gpg").exists() {
                fs::copy(from.join("SHA256SUMS.gpg"), dir.join("SHA256SUMS.gpg"))?;
            }
            verify_signature(dir, keyrings)?;
            println!("Copying from {}...", from.display());
            for name in manifest_files(&fs::read_to_string(dir.join("SHA256SUMS"))?)? {
                fs::copy(from.join(&name), dir.join(&name))?;
            }
        }
    }
    check_hashes(dir)
}

fn download(url: &str, to: &Path) -> Result<()> {
    let to = to.to_str().ok_or_else(|| err("non-UTF-8 path"))?;
    output(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "10",
            "--retry-all-errors",
            "--retry-delay",
            "3",
            "--output",
            to,
            url,
        ],
    )?;
    Ok(())
}

/// The file names listed in a SHA256SUMS manifest ("<hash>  <name>" lines).
/// Anything that is not a plain file name is refused, so a manifest can never
/// make us write outside the download directory.
fn manifest_files(manifest: &str) -> Result<Vec<String>> {
    manifest
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let name = line
                .split_whitespace()
                .nth(1)
                .ok_or_else(|| err(format!("malformed SHA256SUMS line: {line}")))?;
            let name = name.trim_start_matches('*');
            let plain = !name.is_empty()
                && !name.starts_with('.')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._+~-".contains(c));
            if plain {
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
fn check_hashes(dir: &Path) -> Result<()> {
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
fn newest_version(dir: &Path) -> Result<Option<String>> {
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

//! `xctl install-system`: install XeeonOS from the live ISO onto a disk.
//!
//! The ISO carries the system to install in its `xeeonos/` directory: the
//! same files as an update (kernel, /usr and its verity hash tree, compressed,
//! with a SHA256SUMS manifest). Installing means:
//!
//! 1. check the files against the manifest;
//! 2. create a new partition table with systemd-repart: the EFI System
//!    Partition and /usr slot A (verity + data), exactly as in the disk image;
//! 3. decompress /usr and its hash tree straight onto their partitions;
//! 4. install systemd-boot and the kernel into the ESP.
//!
//! Slot B and the data partition are not created here: the installed system
//! creates them itself at its first boot (/usr/lib/repart.d), as it does when
//! started from the disk image, so there is a single source of truth for the
//! layout.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::update::{check_hashes, newest_version};
use crate::util::{Result, err, need_root, output, run};

/// File system label of the live ISO (see tools/mkiso.sh).
const MEDIUM_LABEL: &str = "XEEONOS";
/// Where the system to install is, on the ISO.
const MEDIUM_DIR: &str = "xeeonos";
/// The installed system needs: ESP 1G, two /usr slots of 8G + 128M, and at
/// least 4G of data. Below this size the first boot cannot create its layout.
const MIN_DISK_BYTES: u64 = 24 * 1024 * 1024 * 1024;
/// GPT type of the EFI System Partition.
const ESP_TYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

/// The files of one XeeonOS version, ready to install.
struct Payload {
    version: String,
    uki: PathBuf,
    usr: PathBuf,
    usr_uuid: String,
    verity: PathBuf,
    verity_uuid: String,
}

/// A disk the system can be installed on.
struct Disk {
    path: String, // e.g. /dev/sda
    size: u64,    // bytes
    model: String,
}

pub fn run_install(args: &[String]) -> Result<()> {
    let mut disk_arg = None;
    let mut from = None;
    let mut yes = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--disk" => disk_arg = Some(it.next().ok_or_else(|| err("--disk needs a device"))?),
            "--from" => from = Some(it.next().ok_or_else(|| err("--from needs a directory"))?),
            "--yes" => yes = true,
            other => {
                return Err(err(format!("unknown option for install-system: {other}")));
            }
        }
    }
    need_root("install-system")?;

    // 1. The files to install, checked against their manifest.
    let (dir, _mount) = match from {
        Some(f) => (PathBuf::from(f), None),
        None => {
            let mount = Mount::medium()?;
            (mount.0.join(MEDIUM_DIR), Some(mount))
        }
    };
    let payload = find_payload(&dir)?;
    println!("Checking the files of XeeonOS {}...", payload.version);
    check_hashes(&dir).map_err(|_| err("the installation files are damaged"))?;

    // 2. Which disk?
    let disks = disks()?;
    let disk = match disk_arg {
        Some(path) => disks
            .into_iter()
            .find(|d| &d.path == path)
            .ok_or_else(|| err(format!("{path} is not a disk XeeonOS can be installed on")))?,
        None => choose_disk(disks)?,
    };
    if disk.size < MIN_DISK_BYTES {
        return Err(err(format!(
            "{} is too small: XeeonOS needs at least {}",
            disk.path,
            human(MIN_DISK_BYTES)
        )));
    }
    if !yes {
        confirm(&disk)?;
    }

    // 3. Partitions: ESP and /usr slot A.
    println!("Creating the partitions on {}...", disk.path);
    let defs = TempDir::new("xctl-install-repart")?;
    write_definitions(&defs.0, &payload)?;
    run(
        "systemd-repart",
        &[
            "--empty=force",
            "--dry-run=no",
            &format!("--definitions={}", defs.0.display()),
            &disk.path,
        ],
    )?;
    // Wait until udev has created the device nodes of the new partitions.
    run("udevadm", &["settle"])?;

    // 4. /usr and its verity hash tree.
    println!("Writing the system...");
    write_partition(&payload.verity, &by_partuuid(&payload.verity_uuid))?;
    write_partition(&payload.usr, &by_partuuid(&payload.usr_uuid))?;

    // 5. Boot loader and kernel.
    println!("Installing the boot loader...");
    let esp = esp_partition(&disk.path)?;
    let mnt = TempDir::new("xctl-install-esp")?;
    run("mount", &[&esp, &mnt.0.to_string_lossy()])?;
    let result = install_boot(&mnt.0, &payload);
    let _ = run("umount", &[&mnt.0.to_string_lossy()]);
    result?;

    println!(
        "XeeonOS {} is installed on {}. Remove the installation medium and restart.",
        payload.version, disk.path
    );
    Ok(())
}

/// Find the kernel, /usr and verity files of the (single) version in `dir`.
fn find_payload(dir: &Path) -> Result<Payload> {
    let version = newest_version(dir)
        .map_err(|e| err(format!("cannot read {}: {e}", dir.display())))?
        .ok_or_else(|| err(format!("no XeeonOS version in {}", dir.display())))?;
    let mut usr = None;
    let mut verity = None;
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        // xeeonos_<v>.usr-x86-64[-verity].<uuid>.raw.zst
        let Some(rest) = name.strip_prefix(&format!("xeeonos_{version}.")) else {
            continue;
        };
        let Some(rest) = rest.strip_suffix(".raw.zst") else {
            continue;
        };
        match rest.split_once('.') {
            Some(("usr-x86-64", uuid)) => usr = Some((dir.join(&name), uuid.to_string())),
            Some(("usr-x86-64-verity", uuid)) => verity = Some((dir.join(&name), uuid.to_string())),
            _ => {}
        }
    }
    let missing = |what: &str| err(format!("the {what} of XeeonOS {version} is missing"));
    let (usr, usr_uuid) = usr.ok_or_else(|| missing("/usr partition"))?;
    let (verity, verity_uuid) = verity.ok_or_else(|| missing("verity partition"))?;
    Ok(Payload {
        uki: dir.join(format!("xeeonos_{version}.efi")),
        version,
        usr,
        usr_uuid: dashed_uuid(&usr_uuid)?,
        verity,
        verity_uuid: dashed_uuid(&verity_uuid)?,
    })
}

/// `7581d84d0bdc1119dfef227c25b31ee9` -> `7581d84d-0bdc-1119-dfef-227c25b31ee9`
fn dashed_uuid(hex: &str) -> Result<String> {
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(err(format!("not a partition UUID: {hex}")));
    }
    let h = hex.to_ascii_lowercase();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    ))
}

fn by_partuuid(uuid: &str) -> PathBuf {
    Path::new("/dev/disk/by-partuuid").join(uuid)
}

/// The systemd-repart definitions: the same ESP and slot A as the disk image
/// (see mkosi.conf.d/system/mkosi.extra/usr/lib/repart.d/). The partition
/// UUIDs must be the ones in the file names: the kernel finds its /usr by
/// them (they are derived from the verity root hash).
fn write_definitions(dir: &Path, p: &Payload) -> Result<()> {
    let v = &p.version;
    let files = [
        (
            "00-esp.conf",
            "[Partition]\nType=esp\nFormat=vfat\nSizeMinBytes=1G\nSizeMaxBytes=1G\n".to_string(),
        ),
        (
            "10-usr-verity.conf",
            format!(
                "[Partition]\nType=usr-verity\nLabel=xeeonos_{v}\nUUID={}\nSizeMinBytes=128M\nSizeMaxBytes=128M\n",
                p.verity_uuid
            ),
        ),
        (
            "11-usr.conf",
            format!(
                "[Partition]\nType=usr\nLabel=xeeonos_{v}\nUUID={}\nSizeMinBytes=8G\nSizeMaxBytes=8G\n",
                p.usr_uuid
            ),
        ),
    ];
    for (name, content) in files {
        fs::write(dir.join(name), content)?;
    }
    Ok(())
}

/// Decompress a `.raw.zst` file onto a partition.
fn write_partition(file: &Path, device: &Path) -> Result<()> {
    let target = fs::OpenOptions::new()
        .write(true)
        .open(device)
        .map_err(|e| err(format!("cannot open {}: {e}", device.display())))?;
    // `Stdio::from(file)`: zstd writes straight into the partition.
    let status = Command::new("zstd")
        .args(["--decompress", "--stdout", "--quiet"])
        .arg(file)
        .stdout(Stdio::from(target))
        .status()
        .map_err(|e| err(format!("cannot run zstd: {e}")))?;
    if !status.success() {
        return Err(err(format!("writing {} failed", device.display())));
    }
    // Make sure everything reached the disk.
    fs::File::open(device)?.sync_all()?;
    Ok(())
}

/// The ESP that systemd-repart just created on `disk`.
fn esp_partition(disk: &str) -> Result<String> {
    let list = output(
        "lsblk",
        &["--list", "--noheadings", "--output", "PATH,PARTTYPE", disk],
    )?;
    list.lines()
        .filter_map(|l| l.split_once(char::is_whitespace))
        .find(|(_, t)| t.trim().eq_ignore_ascii_case(ESP_TYPE))
        .map(|(path, _)| path.to_string())
        .ok_or_else(|| err(format!("no EFI System Partition found on {disk}")))
}

/// systemd-boot (also as the fallback loader EFI/BOOT/BOOTX64.EFI, so the
/// disk starts even without a firmware boot entry) and the kernel.
fn install_boot(esp: &Path, p: &Payload) -> Result<()> {
    run(
        "bootctl",
        &["install", &format!("--esp-path={}", esp.display())],
    )?;
    let linux = esp.join("EFI/Linux");
    fs::create_dir_all(&linux)?;
    fs::copy(&p.uki, linux.join(format!("xeeonos_{}.efi", p.version)))?;
    fs::File::open(&linux)?.sync_all()?;
    Ok(())
}

/// Disks that XeeonOS can be installed on: whole, writable disks, except the
/// one the live system was started from.
fn disks() -> Result<Vec<Disk>> {
    let json = output(
        "lsblk",
        &[
            "--json",
            "--bytes",
            "--nodeps",
            "--output",
            "PATH,SIZE,TYPE,RO,MODEL",
        ],
    )?;
    let medium = medium_disk();
    let list: Value =
        serde_json::from_str(&json).map_err(|e| err(format!("cannot read lsblk output: {e}")))?;
    let mut disks = Vec::new();
    for d in list["blockdevices"].as_array().into_iter().flatten() {
        let path = d["path"].as_str().unwrap_or_default().to_string();
        // lsblk prints "ro" as a JSON boolean (newer) or as "0"/"1" (older).
        let read_only = d["ro"].as_bool().unwrap_or(d["ro"] == "1");
        if d["type"] != "disk" || read_only || path.starts_with("/dev/zram") {
            continue;
        }
        if medium.as_deref() == Some(path.as_str()) {
            continue;
        }
        disks.push(Disk {
            path,
            size: d["size"].as_u64().unwrap_or(0),
            model: d["model"].as_str().unwrap_or("").trim().to_string(),
        });
    }
    Ok(disks)
}

/// The disk holding the live medium (a USB stick written with the ISO), if any.
fn medium_disk() -> Option<String> {
    let dev = fs::canonicalize(Path::new("/dev/disk/by-label").join(MEDIUM_LABEL)).ok()?;
    let dev = dev.to_string_lossy().into_owned();
    // The ISO file system is on the whole stick, or on one of its partitions.
    match output(
        "lsblk",
        &["--noheadings", "--nodeps", "--output", "PKNAME", &dev],
    ) {
        Ok(parent) if !parent.trim().is_empty() => Some(format!("/dev/{}", parent.trim())),
        _ => Some(dev),
    }
}

fn choose_disk(disks: Vec<Disk>) -> Result<Disk> {
    if disks.is_empty() {
        return Err(err("no disk found to install XeeonOS on"));
    }
    println!("Disks:");
    for (i, d) in disks.iter().enumerate() {
        println!("  {}) {}  {}  {}", i + 1, d.path, human(d.size), d.model);
    }
    let answer = ask("Install XeeonOS on disk number: ")?;
    let n: usize = answer
        .parse()
        .map_err(|_| err(format!("not a disk number: {answer}")))?;
    // `into_iter().nth(i)` takes the element out of the list (no copy).
    disks
        .into_iter()
        .nth(n.wrapping_sub(1))
        .ok_or_else(|| err(format!("no disk number {n}")))
}

/// Erasing a disk cannot be undone: the user types its name to confirm.
fn confirm(disk: &Disk) -> Result<()> {
    let name = disk.path.trim_start_matches("/dev/");
    println!(
        "\nEVERYTHING on {} ({}, {}) will be erased.",
        disk.path,
        human(disk.size),
        disk.model
    );
    let answer = ask(&format!("Type {name} to continue, anything else to stop: "))?;
    if answer == name {
        Ok(())
    } else {
        Err(err("stopped; nothing was changed"))
    }
}

fn ask(question: &str) -> Result<String> {
    print!("{question}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// 25769803776 -> "24.0 GiB"
fn human(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// A temporary directory, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<TempDir> {
        let path = std::env::temp_dir().join(format!("{name}.{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)?;
        Ok(TempDir(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The live medium, mounted read-only for as long as this value lives.
struct Mount(PathBuf);

impl Mount {
    fn medium() -> Result<Mount> {
        let dev = Path::new("/dev/disk/by-label").join(MEDIUM_LABEL);
        if !dev.exists() {
            return Err(err(
                "the XeeonOS installation medium was not found; use --from <directory>",
            ));
        }
        let dir = PathBuf::from("/run/xctl-medium");
        fs::create_dir_all(&dir)?;
        run(
            "mount",
            &["-o", "ro", &dev.to_string_lossy(), &dir.to_string_lossy()],
        )?;
        Ok(Mount(dir))
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        let _ = run("umount", &[&self.0.to_string_lossy()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid() {
        assert_eq!(
            dashed_uuid("7581d84d0bdc1119dfef227c25b31ee9").unwrap(),
            "7581d84d-0bdc-1119-dfef-227c25b31ee9"
        );
        assert!(dashed_uuid("xyz").is_err());
    }

    #[test]
    fn payload() {
        let dir = std::env::temp_dir().join(format!("xctl-test-payload.{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "xeeonos_1.0.0.efi",
            "xeeonos_1.0.0.usr-x86-64.7581d84d0bdc1119dfef227c25b31ee9.raw.zst",
            "xeeonos_1.0.0.usr-x86-64-verity.41e02eb07aa378f6e8b066d00c62e820.raw.zst",
            "SHA256SUMS",
        ] {
            fs::write(dir.join(name), "").unwrap();
        }
        let p = find_payload(&dir).unwrap();
        assert_eq!(p.version, "1.0.0");
        assert_eq!(p.usr_uuid, "7581d84d-0bdc-1119-dfef-227c25b31ee9");
        assert_eq!(p.verity_uuid, "41e02eb0-7aa3-78f6-e8b0-66d00c62e820");
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! `xctl install-system`: install XeeonOS from the live ISO onto a disk.
//!
//! Three ways, chosen in the first window:
//! - on a whole disk, erasing it;
//! - next to another system, in the free space of its disk: the existing
//!   partitions are not touched (nor resized). The existing EFI System
//!   Partition is used if it has room for the kernels; otherwise a separate
//!   boot partition (XBOOTLDR, 1 GiB) is created in the free space;
//! - manually: the same two, with every choice asked.
//!
//! The ISO carries the system to install in its `xeeonos/` directory: the
//! same files as an update (kernel, /usr and its verity hash tree, compressed,
//! with a SHA256SUMS manifest). Installing means:
//!
//! 1. check the files against the manifest;
//! 2. create the partitions with systemd-repart: the EFI System Partition
//!    (if needed) and /usr slot A (verity + data), exactly as in the disk image;
//! 3. decompress /usr and its hash tree straight onto their partitions;
//! 4. install systemd-boot and the kernel into the ESP.
//!
//! Slot B and the data partition are not created here: the installed system
//! creates them itself at its first boot (/usr/lib/repart.d), as it does when
//! started from the disk image, so there is a single source of truth for the
//! layout.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::ui;
use crate::update::{check_hashes, newest_version};
use crate::util::{Result, err, need_root, output, run};
use crate::{boot, config};

/// File system label of the live ISO (see tools/mkiso.sh).
const MEDIUM_LABEL: &str = "XEEONOS";
/// Where the system to install is, on the ISO.
const MEDIUM_DIR: &str = "xeeonos";
/// The installed system needs: ESP 1G, two /usr slots of 8G + 128M, and at
/// least 4G of data. Below this size the first boot cannot create its layout.
const MIN_DISK_BYTES: u64 = 24 * 1024 * 1024 * 1024;
/// Free space an existing EFI System Partition needs to be used: two
/// kernels (the running version and the next one) and systemd-boot.
const ESP_NEEDS_BYTES: u64 = 300 * 1024 * 1024;
/// GPT types (Discoverable Partitions Specification).
const ESP_TYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const XBOOTLDR_TYPE: &str = "bc13c2ff-59e6-4262-a352-b275fd6f7172";
const USR_VERITY_TYPE: &str = "77ff5f63-e7b6-4633-acf4-1565b864c0e6";
/// Partitions of these types would be taken for XeeonOS's own at its first
/// boot (systemd-repart matches partitions by type): XeeonOS is not
/// installed next to a system that has them.
const OWN_TYPES: &[(&str, &str)] = &[
    ("4f68bce3-e8cd-4db1-96e7-fbcaf984b709", "root"),
    (crate::config::USR_PARTITION_TYPE, "/usr"),
    (USR_VERITY_TYPE, "/usr verity"),
    (XBOOTLDR_TYPE, "boot (XBOOTLDR)"),
];

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
    /// The partitions already on it: (device, GPT type, size).
    parts: Vec<(String, String, u64)>,
    /// GPT partition table (the only kind "next to" works with).
    gpt: bool,
    /// The largest unpartitioned space, in bytes.
    free: u64,
}

impl Disk {
    fn label(&self) -> String {
        format!("{}  {}  {}", self.path, human(self.size), self.model)
    }

    fn esp(&self) -> Option<&str> {
        self.parts
            .iter()
            .find(|p| p.1.eq_ignore_ascii_case(ESP_TYPE))
            .map(|p| p.0.as_str())
    }

    /// Why XeeonOS cannot go into the free space of this disk, if it can't.
    fn free_space_problem(&self) -> Option<String> {
        if !self.gpt {
            return Some(format!(
                "{} has no GPT partition table (an old MBR disk, or empty)",
                self.path
            ));
        }
        if let Some((_, name)) = OWN_TYPES
            .iter()
            .find(|(t, _)| self.parts.iter().any(|p| p.1.eq_ignore_ascii_case(t)))
        {
            return Some(format!(
                "{} already has a {name} partition of another system",
                self.path
            ));
        }
        if self.free < MIN_DISK_BYTES {
            return Some(format!(
                "{} has {} of free space; XeeonOS needs {}",
                self.path,
                human(self.free),
                human(MIN_DISK_BYTES)
            ));
        }
        None
    }
}

/// Where to install, as chosen.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Space {
    /// Erase the whole disk.
    Whole,
    /// Use the free space; the kernels go into the existing EFI System
    /// Partition (`separate_boot: false`, when it has room) or into a new
    /// boot partition.
    Free { separate_boot: bool },
}

/// The boot partitions systemd-repart creates.
#[derive(Clone, Copy, PartialEq, Debug)]
enum NewBoot {
    /// An EFI System Partition (whole disk, or a disk without one).
    Esp,
    /// A boot partition next to an EFI System Partition that is too small.
    Xbootldr,
    /// Nothing: the existing ESP is used.
    None,
}

pub fn run_install(args: &[String]) -> Result<()> {
    let mut disk_arg = None;
    let mut from = None;
    let mut yes = false;
    let mut free_space = false;
    let mut separate_boot = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--disk" => disk_arg = Some(it.next().ok_or_else(|| err("--disk needs a device"))?),
            "--from" => from = Some(it.next().ok_or_else(|| err("--from needs a directory"))?),
            "--yes" => yes = true,
            "--free-space" => free_space = true,
            "--separate-boot" => {
                free_space = true;
                separate_boot = true;
            }
            other => {
                return Err(err(format!("unknown option for install-system: {other}")));
            }
        }
    }
    need_root("install-system")?;
    if !installer_allowed() {
        return Err(err(
            "XeeonOS is already installed and running here. To reinstall it, \
             restart into Recovery (boot menu) or start the installation medium.",
        ));
    }

    // 1. The files to install, checked against their manifest.
    let (dir, _mount) = match from {
        Some(f) => (PathBuf::from(f), None),
        None => {
            let mount = Mount::medium()?;
            (mount.0.join(MEDIUM_DIR), Some(mount))
        }
    };
    let payload = find_payload(&dir)?;
    ui::working(
        "Installing XeeonOS",
        &format!("Checking the files of XeeonOS {}...", payload.version),
    );
    check_hashes(&dir).map_err(|_| err("the installation files are damaged"))?;

    // 2. Which disk, and how?
    let disks = disks()?;
    let (disk, space) = match disk_arg {
        Some(path) => {
            let disk = disks
                .into_iter()
                .find(|d| &d.path == path)
                .ok_or_else(|| err(format!("{path} is not a disk XeeonOS can be installed on")))?;
            let space = if free_space {
                Space::Free { separate_boot }
            } else {
                Space::Whole
            };
            (disk, space)
        }
        None => match choose(disks, &payload.version)? {
            Some(choice) => choice,
            None => return Err(err("stopped; nothing was changed")),
        },
    };
    let new_boot = check_choice(&disk, space)?;
    if !yes && !confirm(&disk, space, new_boot)? {
        return Err(err("stopped; nothing was changed"));
    }

    // 3. Partitions: the boot partition if needed, and /usr slot A.
    ui::working(
        "Installing XeeonOS",
        &format!("Creating the partitions on {}...", disk.path),
    );
    // (The ESP may just have been mounted to measure it: let udev finish.)
    let _ = run("udevadm", &["settle"]);
    match space {
        Space::Whole => repart_whole_disk(&disk.path, &payload)?,
        Space::Free { .. } => append_partitions(&disk.path, &payload, new_boot)?,
    }
    // Wait until udev has created the device nodes of the new partitions.
    run("udevadm", &["settle"])?;

    // 4. /usr and its verity hash tree.
    ui::working("Installing XeeonOS", "Writing the system...");
    write_partition(&payload.verity, &by_partuuid(&payload.verity_uuid))?;
    write_partition(&payload.usr, &by_partuuid(&payload.usr_uuid))?;

    // 5. Boot loader and kernel.
    ui::working("Installing XeeonOS", "Installing the boot loader...");
    let esp = partition_of_type(&disk.path, ESP_TYPE)?;
    let esp_dir = Mounted::new(&esp, "xctl-install-esp")?;
    let boot_dir = match new_boot {
        NewBoot::Xbootldr => Some(Mounted::new(
            &partition_of_type(&disk.path, XBOOTLDR_TYPE)?,
            "xctl-install-boot",
        )?),
        _ => None,
    };
    install_boot(
        esp_dir.path(),
        boot_dir.as_ref().map(Mounted::path),
        &payload,
    )?;
    drop(boot_dir);
    drop(esp_dir);

    let done = format!(
        "XeeonOS {} is installed on {}. Remove the installation medium and restart.",
        payload.version, disk.path
    );
    if ui::interactive() {
        ui::message("Installing XeeonOS", &done)?;
    }
    println!("{done}");
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

/// The whole disk: a new partition table, made by systemd-repart.
fn repart_whole_disk(disk: &str, payload: &Payload) -> Result<()> {
    let defs = TempDir::new("xctl-install-repart")?;
    write_definitions(&defs.0, payload, NewBoot::Esp)?;
    // SYSTEMD_LOG_TARGET=console: its messages on the screen, also when the
    // installer runs as a service (tests), not only in the journal.
    let status = Command::new("systemd-repart")
        .args([
            "--empty=force",
            "--dry-run=no",
            &format!("--definitions={}", defs.0.display()),
            disk,
        ])
        .env("SYSTEMD_LOG_TARGET", "console")
        .status()
        .map_err(|e| err(format!("cannot run systemd-repart: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(err(format!(
            "creating the partitions on {disk} failed (see the messages above)"
        )))
    }
}

/// Next to another system: the new partitions go into the free space with
/// `sfdisk --append`, which never touches the existing ones. (systemd-repart
/// refuses to place new partitions after partitions it does not manage.)
fn append_partitions(disk: &str, p: &Payload, boot: NewBoot) -> Result<()> {
    let v = &p.version;
    let mut script = String::new();
    match boot {
        NewBoot::Esp => script.push_str(&format!("size=1GiB, type={ESP_TYPE}, name=\"ESP\"\n")),
        NewBoot::Xbootldr => script.push_str(&format!(
            "size=1GiB, type={XBOOTLDR_TYPE}, name=\"XBOOTLDR\"\n"
        )),
        NewBoot::None => {}
    }
    script.push_str(&format!(
        "size=128MiB, type={USR_VERITY_TYPE}, uuid={}, name=\"xeeonos_{v}\"\n",
        p.verity_uuid
    ));
    script.push_str(&format!(
        "size=8GiB, type={}, uuid={}, name=\"xeeonos_{v}\"\n",
        config::USR_PARTITION_TYPE,
        p.usr_uuid
    ));
    let mut child = Command::new("sfdisk")
        .args(["--append", "--quiet", disk])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| err(format!("cannot run sfdisk: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(script.as_bytes())?;
    }
    if !child.wait()?.success() {
        return Err(err(format!("creating the partitions on {disk} failed")));
    }
    run("udevadm", &["settle"])?;
    // The new boot partition needs a file system (FAT, as the ESP).
    let (kind, label) = match boot {
        NewBoot::Esp => (ESP_TYPE, "ESP"),
        NewBoot::Xbootldr => (XBOOTLDR_TYPE, "XBOOTLDR"),
        NewBoot::None => return Ok(()),
    };
    let part = partition_of_type(disk, kind)?;
    run("mkfs.vfat", &["-F", "32", "-n", label, &part])
}

/// The systemd-repart definitions: the same ESP and slot A as the disk image
/// (see mkosi.conf.d/system/mkosi.extra/usr/lib/repart.d/). The partition
/// UUIDs must be the ones in the file names: the kernel finds its /usr by
/// them (they are derived from the verity root hash).
fn write_definitions(dir: &Path, p: &Payload, boot: NewBoot) -> Result<()> {
    let v = &p.version;
    let boot_file = match boot {
        NewBoot::Esp => Some((
            "00-esp.conf",
            "[Partition]\nType=esp\nFormat=vfat\nSizeMinBytes=1G\nSizeMaxBytes=1G\n".to_string(),
        )),
        NewBoot::Xbootldr => Some((
            "01-xbootldr.conf",
            "[Partition]\nType=xbootldr\nFormat=vfat\nSizeMinBytes=1G\nSizeMaxBytes=1G\n"
                .to_string(),
        )),
        NewBoot::None => None,
    };
    let files = [
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
    for (name, content) in boot_file.into_iter().chain(files) {
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

/// The first partition of GPT type `kind` on `disk` (the ESP or the boot
/// partition, just created or already there).
fn partition_of_type(disk: &str, kind: &str) -> Result<String> {
    let list = output(
        "lsblk",
        &["--list", "--noheadings", "--output", "PATH,PARTTYPE", disk],
    )?;
    list.lines()
        .filter_map(|l| l.split_once(char::is_whitespace))
        .find(|(_, t)| t.trim().eq_ignore_ascii_case(kind))
        .map(|(path, _)| path.to_string())
        .ok_or_else(|| err(format!("no partition of type {kind} found on {disk}")))
}

/// systemd-boot (also as the fallback loader EFI/BOOT/BOOTX64.EFI, so the
/// disk starts even without a firmware boot entry) and the kernel, into the
/// boot partition if there is one, else into the ESP. systemd-boot also
/// shows the other systems it finds (e.g. Windows).
fn install_boot(esp: &Path, boot: Option<&Path>, p: &Payload) -> Result<()> {
    let mut args = vec![
        "install".to_string(),
        format!("--esp-path={}", esp.display()),
    ];
    if let Some(b) = boot {
        args.push(format!("--boot-path={}", b.display()));
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run("bootctl", &args)?;
    let linux = boot.unwrap_or(esp).join("EFI/Linux");
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
            "PATH,SIZE,TYPE,RO,MODEL,PTTYPE",
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
            parts: partitions(&path),
            free: largest_free(&path),
            gpt: d["pttype"] == "gpt",
            size: d["size"].as_u64().unwrap_or(0),
            model: d["model"].as_str().unwrap_or("").trim().to_string(),
            path,
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

/// The partitions on `disk`: (device, GPT type, size).
fn partitions(disk: &str) -> Vec<(String, String, u64)> {
    let Ok(json) = output(
        "lsblk",
        &[
            "--json",
            "--bytes",
            "--output",
            "PATH,TYPE,PARTTYPE,SIZE",
            disk,
        ],
    ) else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Value>(&json) else {
        return Vec::new();
    };
    list["blockdevices"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|d| d["children"].as_array().cloned().unwrap_or_default())
        .filter(|p| p["type"] == "part")
        .map(|p| {
            (
                p["path"].as_str().unwrap_or_default().to_string(),
                p["parttype"].as_str().unwrap_or_default().to_string(),
                p["size"].as_u64().unwrap_or(0),
            )
        })
        .collect()
}

/// The largest unpartitioned area of `disk`, in bytes, from `sfdisk
/// --list-free`.
fn largest_free(disk: &str) -> u64 {
    output("sfdisk", &["--list-free", disk])
        .map(|text| parse_free(&text))
        .unwrap_or(0)
}

/// `sfdisk --list-free` prints the sector size, then one line per free area:
/// "Start End Sectors Size".
fn parse_free(text: &str) -> u64 {
    let sector: u64 = text
        .lines()
        .find(|l| l.starts_with("Units:"))
        .and_then(|l| l.rsplit('=').next())
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(512);
    text.lines()
        .skip_while(|l| !l.trim_start().starts_with("Start"))
        .skip(1)
        .filter_map(|l| l.split_whitespace().nth(2)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        * sector
}

/// The windows of the installer: how, which disk, which boot partition.
/// `None`: the user went back from the first window.
fn choose(disks: Vec<Disk>, version: &str) -> Result<Option<(Disk, Space)>> {
    if disks.is_empty() {
        return Err(err("no disk found to install XeeonOS on"));
    }
    if !ui::interactive() {
        return Err(err("give the disk with --disk (no terminal to ask on)"));
    }
    loop {
        let how = [
            ("whole", "Erase a disk and install XeeonOS"),
            ("free", "Install next to another system (in the free space)"),
            (
                "manual",
                "Manual: choose the disk, the space and the boot partition",
            ),
        ];
        let items: Vec<(String, String)> = how
            .iter()
            .map(|(t, d)| (t.to_string(), d.to_string()))
            .collect();
        let Some(mode) = ui::menu(
            &format!("Install XeeonOS {version}"),
            "How do you want to install XeeonOS?",
            &items,
            None,
        )?
        else {
            return Ok(None);
        };
        // Disks that fit the chosen way.
        let fits = |d: &Disk| match mode.as_str() {
            "whole" => d.size >= MIN_DISK_BYTES,
            "free" => d.free_space_problem().is_none(),
            _ => true,
        };
        let items: Vec<(String, String)> = disks
            .iter()
            .filter(|d| fits(d))
            .map(|d| {
                let what = if d.parts.is_empty() {
                    "empty".to_string()
                } else {
                    format!("{} partitions, {} free", d.parts.len(), human(d.free))
                };
                (d.path.clone(), format!("{}  ({what})", d.label()))
            })
            .collect();
        if items.is_empty() {
            let why: Vec<String> = disks
                .iter()
                .filter_map(|d| d.free_space_problem())
                .collect();
            ui::message(
                "Install XeeonOS",
                &format!(
                    "No disk fits. XeeonOS needs {} on a whole disk, or {} of free \
                     space next to another system.\n\n{}",
                    human(MIN_DISK_BYTES),
                    human(MIN_DISK_BYTES),
                    why.join("\n")
                ),
            )?;
            continue;
        }
        let Some(path) = ui::menu("Install XeeonOS", "Choose the disk:", &items, None)? else {
            continue;
        };
        let Some(disk) = disks.iter().position(|d| d.path == path) else {
            continue;
        };
        let space = match mode.as_str() {
            "whole" => Space::Whole,
            "free" => Space::Free {
                separate_boot: false,
            },
            _ => match manual(&disks[disk])? {
                Some(space) => space,
                None => continue,
            },
        };
        // `swap_remove`: take the chosen disk out of the list (no copy).
        let mut disks = disks;
        return Ok(Some((disks.swap_remove(disk), space)));
    }
}

/// Manual: the whole disk or its free space, and where the kernels go.
fn manual(disk: &Disk) -> Result<Option<Space>> {
    let mut items = vec![(
        "whole".to_string(),
        format!("Erase the whole disk ({})", human(disk.size)),
    )];
    match disk.free_space_problem() {
        None => items.push((
            "free".to_string(),
            format!(
                "Use the free space ({}); keep the other partitions",
                human(disk.free)
            ),
        )),
        Some(why) => items.push(("cannot".to_string(), format!("(Free space: {why})"))),
    }
    let Some(choice) = ui::menu("Manual installation", &disk.label(), &items, None)? else {
        return Ok(None);
    };
    match choice.as_str() {
        "whole" => Ok(Some(Space::Whole)),
        "free" if disk.esp().is_none() => Ok(Some(Space::Free {
            separate_boot: false,
        })),
        "free" => {
            let items = vec![
                (
                    "esp".to_string(),
                    format!(
                        "Use the existing EFI System Partition (needs {} free)",
                        human(ESP_NEEDS_BYTES)
                    ),
                ),
                (
                    "boot".to_string(),
                    "Create a separate boot partition (1 GiB of the free space)".to_string(),
                ),
            ];
            Ok(ui::menu(
                "Manual installation",
                "Where should the boot loader and the kernels go?",
                &items,
                Some("esp"),
            )?
            .map(|c| Space::Free {
                separate_boot: c == "boot",
            }))
        }
        _ => Ok(None),
    }
}

/// Check that the choice works on this disk, and decide which boot
/// partition to create.
fn check_choice(disk: &Disk, space: Space) -> Result<NewBoot> {
    match space {
        Space::Whole => {
            if disk.size < MIN_DISK_BYTES {
                return Err(err(format!(
                    "{} is too small: XeeonOS needs at least {}",
                    disk.path,
                    human(MIN_DISK_BYTES)
                )));
            }
            Ok(NewBoot::Esp)
        }
        Space::Free { separate_boot } => {
            if let Some(why) = disk.free_space_problem() {
                return Err(err(why));
            }
            let Some(esp) = disk.esp() else {
                return Ok(NewBoot::Esp);
            };
            if separate_boot {
                return Ok(NewBoot::Xbootldr);
            }
            let free = esp_free(esp)?;
            Ok(if free >= ESP_NEEDS_BYTES {
                NewBoot::None
            } else {
                NewBoot::Xbootldr
            })
        }
    }
}

/// Free bytes in an existing EFI System Partition (mounted read-only).
fn esp_free(esp: &str) -> Result<u64> {
    let mnt = TempDir::new("xctl-install-esp-check")?;
    run("mount", &["-o", "ro", esp, &mnt.0.to_string_lossy()])?;
    let df = output(
        "df",
        &["--output=avail", "--block-size=1", &mnt.0.to_string_lossy()],
    );
    let _ = run("umount", &[&mnt.0.to_string_lossy()]);
    df?.lines()
        .nth(1)
        .and_then(|l| l.trim().parse().ok())
        .ok_or_else(|| err(format!("cannot read the free space of {esp}")))
}

/// What is going to happen, and the user's go. Erasing a disk cannot be
/// undone: the user types its name.
fn confirm(disk: &Disk, space: Space, boot: NewBoot) -> Result<bool> {
    if !ui::interactive() {
        return Err(err("give --yes to install without a terminal to ask on"));
    }
    match space {
        Space::Whole => {
            let name = disk.path.trim_start_matches("/dev/");
            let answer = ui::input(
                "Erase the disk",
                &format!(
                    "EVERYTHING on {} will be erased.\n\nType {name} to continue:",
                    disk.label()
                ),
                "",
            )?;
            Ok(answer.as_deref() == Some(name))
        }
        Space::Free { .. } => {
            let boot = match boot {
                NewBoot::Esp => "a new EFI System Partition (1 GiB)",
                NewBoot::Xbootldr => {
                    "a boot partition (1 GiB); the existing EFI System Partition gets \
                     only systemd-boot"
                }
                NewBoot::None => {
                    "the existing EFI System Partition (for systemd-boot and the kernels)"
                }
            };
            ui::yes_no(
                "Install next to another system",
                &format!(
                    "XeeonOS goes into the free space of {}. The other partitions are \
                     not changed.\n\nBoot: {boot}.\n\nThe boot menu (systemd-boot) will \
                     also offer the other system. Install?",
                    disk.label()
                ),
                true,
            )
        }
    }
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

/// A partition mounted on a temporary directory while this value lives.
/// (Unmounted first, then the directory is removed.)
struct Mounted(TempDir);

impl Mounted {
    fn new(device: &str, name: &str) -> Result<Mounted> {
        let dir = TempDir::new(name)?;
        run("mount", &[device, &dir.0.to_string_lossy()])?;
        Ok(Mounted(dir))
    }

    fn path(&self) -> &Path {
        &self.0.0
    }
}

impl Drop for Mounted {
    fn drop(&mut self) {
        let _ = run("umount", &[&self.path().to_string_lossy()]);
    }
}

/// The installer runs only from the live ISO or from Recovery, never on a
/// running installed system (it would erase the disk it is running from).
fn installer_allowed() -> bool {
    Path::new(config::LIVE_MARKER).exists() || boot::in_recovery()
}

/// The live medium, mounted read-only for as long as this value lives.
struct Mount(PathBuf);

impl Mount {
    fn medium() -> Result<Mount> {
        let dev = Path::new("/dev/disk/by-label").join(MEDIUM_LABEL);
        // Early in the boot, udev may not have named the disks yet: wait
        // for it, and give a slow drive (USB, DVD) up to 30 seconds.
        let _ = run("udevadm", &["settle", "--timeout=30"]);
        for _ in 0..30 {
            if dev.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
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
    fn free_space() {
        let text = "Unpartitioned space /dev/sda: 27.8 GiB, 29827260416 bytes, 58256368 sectors
Units: sectors of 1 * 512 = 512 bytes
Sector size (logical/physical): 512 bytes / 512 bytes

   Start      End  Sectors  Size
    2048   411647   409600  200M
 4605952 62914526 58308575 27.8G
";
        assert_eq!(parse_free(text), 58308575 * 512);
        assert_eq!(parse_free(""), 0);
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

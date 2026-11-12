//! `xctl status`: what is running, and is the system sealed?

use std::fs;
use std::path::Path;

use crate::boot;
use crate::channel;
use crate::config;
use crate::util::{Result, err, output_opt};
use crate::version::read_os_release;

/// A /usr slot (partition) on the system disk.
struct Slot {
    device: String, // e.g. "vda3"
    label: String,  // e.g. "xeeonos_0.1.0-experimental.42", or "_empty"
}

pub fn run(check: bool) -> Result<()> {
    let os = read_os_release(config::OS_RELEASE)?;
    let version = os.get("IMAGE_VERSION").map_or("unknown", String::as_str);
    let build = os.get("BUILD_ID").map_or("unknown", String::as_str);
    let channel = channel::current()?;

    // How is /usr mounted? On a sealed system it is a dm-verity device
    // (/dev/mapper/usr), read-only.
    let usr_source =
        output_opt("findmnt", &["--noheadings", "--output", "SOURCE", "/usr"]).unwrap_or_default();
    let usr_options =
        output_opt("findmnt", &["--noheadings", "--output", "OPTIONS", "/usr"]).unwrap_or_default();
    let usr_read_only = usr_options.split(',').any(|o| o == "ro");

    let mut usr_verity = false;
    let mut running_slot = None;
    // Read the device-mapper details from /sys, which (unlike `dmsetup`)
    // works without root: /dev/mapper/usr -> /dev/dm-N -> /sys/block/dm-N.
    if usr_source.starts_with("/dev/mapper/")
        && let Ok(dev) = fs::canonicalize(&usr_source)
        && let Some(dm) = dev.file_name().and_then(|n| n.to_str())
    {
        let sys = Path::new("/sys/block").join(dm);
        // cryptsetup names verity devices "CRYPT-VERITY-...".
        usr_verity = fs::read_to_string(sys.join("dm/uuid"))
            .is_ok_and(|uuid| uuid.starts_with("CRYPT-VERITY-"));
        running_slot = usr_data_partition(&sys);
    }

    let root_fs =
        output_opt("findmnt", &["--noheadings", "--output", "FSTYPE", "/"]).unwrap_or_default();

    println!("XeeonOS {version} (build {build})");
    if let Some(snapshot) = os.get("XEEONOS_ARCH_SNAPSHOT") {
        println!("Based on:    Arch Linux (snapshot {snapshot})");
    }
    println!(
        "Channel:     {}",
        channel.as_deref().unwrap_or("none (development build)")
    );
    println!(
        "/usr:        {usr_source} (read-only: {}, verity: {})",
        yes_no(usr_read_only),
        yes_no(usr_verity)
    );
    println!("Data (/):    {root_fs}");
    // Without root rights the LUKS header cannot be read: say only whether
    // the data partition is encrypted.
    let encryption = match crate::encryption::data_partition() {
        Err(_) => "no".to_string(),
        Ok(dev) if crate::util::is_root() => match crate::encryption::password_set(&dev) {
            Ok(true) => "yes, password on".to_string(),
            Ok(false) => "yes, password off (opens by itself)".to_string(),
            Err(_) => "yes".to_string(),
        },
        Ok(_) => "yes".to_string(),
    };
    println!("Encrypted:   {encryption}");
    if let Some(v) = crate::update::available() {
        println!("Update:      XeeonOS {v} is available (sudo xctl update)");
    }
    if let Some(last) = crate::snapshots::list()
        .ok()
        .and_then(|l| l.last().cloned())
    {
        println!("Snapshot:    {last} (newest; see xctl snapshots)");
    }

    let mut slots = Vec::new();
    // `if let ... && let ...` ("let chains"): both must match.
    if let Some(running) = &running_slot
        && let Some(disk) = output_opt(
            "lsblk",
            &[
                "--noheadings",
                "--nodeps",
                "--output",
                "PKNAME",
                &format!("/dev/{running}"),
            ],
        )
    {
        slots = usr_slots(&disk);
        println!("/usr slots on /dev/{disk}:");
        for slot in &slots {
            let label = if slot.label == "_empty" {
                "(empty)"
            } else {
                &slot.label
            };
            let mark = if &slot.device == running {
                "  (running)"
            } else {
                ""
            };
            println!("  /dev/{}  {label}{mark}", slot.device);
        }
    }

    // The boot menu: one kernel per version, with its boot-counting state.
    if let Ok(entries) = boot::entries(&boot::esp()) {
        println!("Boot menu (newest first):");
        let width = entries.iter().map(|e| e.version.len()).max().unwrap_or(0);
        for e in &entries {
            let running = if e.version == version {
                ", running"
            } else {
                ""
            };
            // `{:<width$}` pads the version to the same width on every line.
            println!("  {:<width$}  {}{running}", e.version, e.state());
        }
    }

    if check {
        let mut problems = Vec::new();
        if !usr_read_only {
            problems.push("/usr is not read-only".to_string());
        }
        if !usr_verity {
            problems.push("/usr is not protected by dm-verity".to_string());
        }
        if root_fs != "btrfs" {
            problems.push("the data partition is not Btrfs".to_string());
        }
        if slots.len() < 2 {
            problems.push(format!("expected 2 /usr slots, found {}", slots.len()));
        }
        if !problems.is_empty() {
            for p in &problems {
                println!("check: {p}");
            }
            return Err(err("the system is not sealed"));
        }
        println!("check: sealed system OK");
    }
    Ok(())
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// The verity device sits on two partitions (data and hash tree); return the
/// data one, i.e. the partition whose GPT type is /usr.
fn usr_data_partition(sys: &Path) -> Option<String> {
    // /sys/block/dm-N/slaves/ lists the partitions under the device.
    let mut devs: Vec<String> = fs::read_dir(sys.join("slaves"))
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    devs.sort();
    devs.into_iter().find(|dev| {
        output_opt(
            "lsblk",
            &[
                "--noheadings",
                "--nodeps",
                "--output",
                "PARTTYPE",
                &format!("/dev/{dev}"),
            ],
        )
        .is_some_and(|t| t.eq_ignore_ascii_case(config::USR_PARTITION_TYPE))
    })
}

/// All /usr partitions on a disk, in disk order.
fn usr_slots(disk: &str) -> Vec<Slot> {
    let list = output_opt(
        "lsblk",
        &[
            "--list",
            "--noheadings",
            "--output",
            "NAME,PARTTYPE,PARTLABEL",
            &format!("/dev/{disk}"),
        ],
    )
    .unwrap_or_default();
    list.lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let (device, ptype) = (cols.next()?, cols.next()?);
            let label = cols.next().unwrap_or("");
            ptype
                .eq_ignore_ascii_case(config::USR_PARTITION_TYPE)
                .then(|| Slot {
                    device: device.to_string(),
                    label: label.to_string(),
                })
        })
        .collect()
}

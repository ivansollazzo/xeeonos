//! `xctl report`: everything needed to understand a problem, in one file to
//! attach to a bug report: versions, boot entries, disks, network, failed
//! services, disk health, kernel crashes and the logs of this boot and of
//! the previous one.
//!
//! Nothing secret is collected (no passwords, keys or users' files); the
//! file is readable only by the administrator who asked for it.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::util::{Result, err, need_root, output};

/// What goes into the report: (file name, program, arguments).
const PARTS: &[(&str, &str, &[&str])] = &[
    ("status.txt", "xctl", &["status"]),
    ("os-release.txt", "cat", &["/usr/lib/os-release"]),
    ("boot-entries.txt", "bootctl", &["list", "--no-pager"]),
    (
        "sysupdate.txt",
        "/usr/lib/systemd/systemd-sysupdate",
        &["list"],
    ),
    (
        "disks.txt",
        "lsblk",
        &[
            "--fs",
            "--output",
            "NAME,SIZE,TYPE,FSTYPE,LABEL,PARTTYPENAME,MOUNTPOINTS",
        ],
    ),
    ("space.txt", "df", &["--human-readable"]),
    (
        "failed-services.txt",
        "systemctl",
        &["--failed", "--no-pager"],
    ),
    ("network.txt", "nmcli", &["device", "status"]),
    ("firewall.txt", "xctl", &["firewall", "status"]),
    ("snapshots.txt", "xctl", &["snapshots"]),
    ("health.txt", "xctl", &["health"]),
    (
        "smart.txt",
        "sh",
        &[
            "-c",
            "for d in $(lsblk -dno NAME,TYPE | awk '$2 == \"disk\" && $1 !~ /^zram/ {print $1}'); do smartctl --all /dev/$d; done",
        ],
    ),
    // Saved by systemd-pstore from the firmware after a kernel crash.
    (
        "crashes.txt",
        "sh",
        &[
            "-c",
            "find /var/lib/systemd/pstore -type f -exec sh -c 'echo \"== $1\"; cat \"$1\"' _ {} \\; 2>/dev/null; true",
        ],
    ),
    ("hardware.txt", "lspci", &["-nn"]),
    (
        "kernel.txt",
        "journalctl",
        &["--dmesg", "--boot", "--no-pager"],
    ),
    (
        "journal-this-boot.txt",
        "journalctl",
        &["--boot", "--no-pager", "--priority=info"],
    ),
    (
        "journal-previous-boot.txt",
        "journalctl",
        &["--boot=-1", "--no-pager", "--priority=info"],
    ),
    ("packages.txt", "xctl", &["packages"]),
];

pub fn run_report(args: &[String]) -> Result<()> {
    if let Some(arg) = args.first() {
        return Err(err(format!("unknown option for report: {arg}")));
    }
    need_root("report")?;

    let date = output("date", &["+%Y-%m-%d_%H%M%S"])?;
    let name = format!("xeeonos-report-{}", date.trim());
    let base = Path::new("/var/tmp");
    let dir = base.join(&name);
    fs::create_dir_all(&dir)?;

    for (file, program, args) in PARTS {
        // A part that cannot be collected says why, instead of stopping the report.
        let text = match Command::new(program).args(*args).output() {
            Ok(out) => {
                let mut t = String::from_utf8_lossy(&out.stdout).into_owned();
                t.push_str(&String::from_utf8_lossy(&out.stderr));
                t
            }
            Err(e) => format!("cannot run {program}: {e}\n"),
        };
        fs::write(dir.join(file), text)?;
    }

    let archive = base.join(format!("{name}.tar.zst"));
    let ok = Command::new("tar")
        .arg("--zstd")
        .arg("--create")
        .arg("--file")
        .arg(&archive)
        .arg("--directory")
        .arg(base)
        .arg(&name)
        .status()
        .map_err(|e| err(format!("cannot run tar: {e}")))?
        .success();
    let _ = fs::remove_dir_all(&dir);
    if !ok {
        return Err(err("could not write the report"));
    }
    // Only the administrator can read it (the logs can be personal).
    let _ = Command::new("chmod").arg("600").arg(&archive).status();
    println!("Report written to {}", archive.display());
    println!("Attach it to the bug report. (No passwords, keys or personal files are in it.)");
    Ok(())
}

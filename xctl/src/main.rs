//! xctl - the XeeonOS system tool.
//!
//! Commands: `status`, `update`, `rollback`, `channel`, `packages`, `setup`,
//! `ext`, `install-system`, `recovery`, `network`, `encryption`, `firewall`, `snapshots`,
//! `report`. Each command lives in its own module (file) below.

mod apps;
mod boot;
mod channel;
mod encryption;
mod ext;
mod firewall;
mod health;
mod install;
mod network;
mod oci;
mod packages;
mod recovery;
mod report;
mod secureboot;
mod setup;
mod snapshots;
mod ssh;
mod status;
mod ui;
mod update;
mod util;
mod version;

/// Fixed paths and names, in one place.
mod config {
    /// Where updates are published: one OCI artifact per version, tagged with
    /// the exact version and with its channel.
    pub const REGISTRY: &str = "ghcr.io/ivansollazzo/xeeonos";
    /// Where the optional extensions are published (the repository
    /// xeeonos-extensions): one OCI artifact per XeeonOS version, tagged
    /// with that version, signed with the same key.
    pub const EXT_REGISTRY: &str = "ghcr.io/ivansollazzo/xeeonos-extensions";
    /// Public keys allowed to sign updates (OpenPGP keyring, as `gpg --export`).
    pub const KEYRING: &str = "/usr/lib/xeeonos/update-keyring.gpg";
    /// Download directory; the sysupdate.d transfer files read from here.
    pub const CACHE: &str = "/var/cache/xctl/update";
    /// Written by `xctl update --check` (as root) when a newer version is
    /// available: shown at login and by `xctl status`.
    pub const UPDATE_AVAILABLE: &str = "/var/lib/xeeonos/update-available";
    /// Disk problems found by the last `xctl health` (xeeonos-health.timer).
    pub const HEALTH: &str = "/var/lib/xeeonos/health";
    /// Update settings written by the administrator (docs/updates.md).
    pub const UPDATE_CONF: &str = "/etc/xeeonos/update.conf";
    /// The channel chosen by the administrator, if any.
    pub const CHANNEL_FILE: &str = "/etc/xeeonos/channel";
    pub const OS_RELEASE: &str = "/usr/lib/os-release";
    /// System extensions (xctl extensions): what this version knows (<name>.conf)
    /// and the images included in the system image (<name>.xext).
    pub const EXTENSIONS: &str = "/usr/lib/xeeonos/extensions";
    /// Downloaded extensions: <version>/<name>.xext and <name>.conf.
    pub const EXTENSIONS_STORE: &str = "/var/lib/xeeonos/extensions";
    /// Extensions installed from a file: <name>.xext and <name>.conf.
    pub const EXTENSIONS_LOCAL: &str = "/var/lib/xeeonos/extensions-local";
    /// The administrator's choices: <name>=on|off.
    pub const EXTENSIONS_CHOICES: &str = "/etc/xeeonos/extensions";
    /// Where systemd-sysext looks for the extensions to lay over /usr.
    pub const EXTENSIONS_RUN: &str = "/run/extensions";
    pub const EXTENSIONS_CACHE: &str = "/var/cache/xctl/ext";
    /// One empty file per extension whose Setup= script has run.
    pub const EXTENSIONS_SETUP: &str = "/var/lib/xeeonos/extensions-setup";
    /// Present only in the live ISO (mkosi.profiles/live).
    pub const LIVE_MARKER: &str = "/usr/lib/xeeonos/live";
    /// Kernel command line word of the Recovery boot entry.
    pub const RECOVERY_CMDLINE: &str = "xeeonos.recovery";
    pub const SYSUPDATE: &str = "/usr/lib/systemd/systemd-sysupdate";
    /// GPT partition type of /usr on x86-64 (Discoverable Partitions Specification).
    pub const USR_PARTITION_TYPE: &str = "8484680c-9521-48c6-9c11-b0720656f69e";
}

const USAGE: &str = "\
Usage: xctl <command> [options]

Commands:
  status [--check]       Show the version, the channel and the state of the
                         sealed system. With --check, exit with an error if
                         the system is not sealed.
  update [options]       Download, verify and install the newest version of
                         the channel into the free /usr slot. The running
                         system is untouched; the new version starts at the
                         next restart.
      --from <source>    Install from <source> instead of the channel:
                         a directory (e.g. a USB stick), an http(s) URL of a
                         directory, or oci://<registry>/<name>:<tag>.
      --keyring <file>   Also accept updates signed by the keys in <file>.
      --check            Only say whether a newer version is available
                         (downloads only the small signed manifest; no root).
  rollback [--yes]       Go back to the previous version at the next restart
                         (asks first; the version left is turned off until
                         the next `xctl update`).
                         The running version is marked as bad and will not
                         be started (or installed) again.
  channel                Show the update channel.
  channel list           List the channels (needs the network): stable
                         (fixed versions YY.MM.S), testing (what comes
                         next).
  channel set <name>     Switch to the channel <name> (only one that exists).
  channel reset          Go back to the channel this version was built for.
  extensions [list]      List the system extensions (software added on top
                         of the sealed system, see docs/extensions.md): the
                         installed ones, and those that can be downloaded
                         for this version (needs the network).
  extensions enable <name>
                         Turn an extension on (an optional one is downloaded
                         first), at once (one with kernel modules: from the
                         next restart). Offers to turn on the extensions it
                         needs, and to turn off those it cannot be on with.
      --from <source>    Download it from <source> instead of the registry
                         (a directory, an http(s) URL, or oci://...).
      --keyring <file>   Also accept the keys in <file>.
      --yes              Do not ask (turn on what it needs, off what it
                         cannot be on with).
      --no-apply         Only from the next restart.
  extensions disable <name> [--yes] [--no-apply]
                         Turn an extension off, at once (with --no-apply, or
                         one with kernel modules: from the next restart). A
                         special one asks first; so does one that others
                         need (they are turned off too).
  extensions install <file> [--yes] [--no-apply]
                         Install an extension image from a file (not signed,
                         not verified: asks first) and turn it on.
  extensions remove <name> [--yes]
                         Turn an optional extension off and delete it (and
                         turn off those that need it, after asking).
  ssh-server [on|off]    Show, or turn on or off, the SSH server (and open or
                         close its port, 22/tcp, in the firewall).
  packages [<name>]      List the packages of this version (and their
                         versions); with <name>, only those containing it.
  setup                  First-boot setup: language, keyboard, time zone,
                         computer name, first user. It runs by itself at the
                         first boot (and after erasing everything), and
                         never again on a system already set up.
  install-system [options]
                         Install XeeonOS onto a disk, from the live ISO or
                         from Recovery: on a whole disk (erased), or next
                         to another system in its free space (asked in
                         dialog windows if --disk is not given).
      --disk <device>    The disk, e.g. /dev/sda.
      --free-space       Use the free space of the disk; keep the other
                         partitions.
      --separate-boot    With --free-space: put the kernels in a new boot
                         partition, even if the EFI partition has room.
      --encrypt          Ask for a disk password at the first start.
      --from <source>    Install from <source> instead of the installation
                         medium: a directory, a web directory
                         (http[s]://...) or a registry (oci://...). The
                         network ISO, which carries no system, downloads
                         it from its channel (needs the network).
      --keyring <file>   Also accept downloads signed by the keys in <file>.
      --yes              Do not ask for confirmation (for automated tests).
  network [<what>]       Network settings: without <what>, a screen to
                         choose a Wi-Fi network or set up a wired one.
      status             Show the network devices and their addresses.
      ethernet [--device <dev>] (--dhcp | --address <ip>/<prefix>
               --gateway <ip> [--dns <ip>,...])
      wifi <name> [--password <p>] [--dhcp | --address ... ]
  encryption [status|on|off]
                         The disk password of the data partition (always
                         encrypted). on: a password at every start and a
                         recovery key, shown once. off: no password.
      --reencrypt        With on: also re-encrypt everything (slow).
      --password-stdin   Read the password from standard input.
  encryption tpm on|off  With the password on: let this computer's TPM open
                         the disk by itself while Secure Boot is unchanged
                         (the password and recovery key still work).
  recovery               Restart into Recovery (asks first). In Recovery,
                         show the Recovery menu: reinstall, previous version,
                         restore data, network, shell, erase everything,
                         restart.
  firewall [status]      Show the firewall (closed to incoming connections).
  firewall allow|deny <port>/<tcp|udp>
                         Open or close a port for incoming connections.
  snapshots [list]       Show the copies of the data (settings and users'
                         files) taken before every update.
  snapshots create       Take one now.
  snapshots restore <name> [--yes]
                         In Recovery: bring back settings and files as they
                         were in that snapshot (the current state is saved
                         first).
  health                 Is the disk in good shape? (Btrfs errors, SMART;
                         checked daily.)
  report                 Write a report for a bug (versions, disks, logs)
                         to /var/tmp, to attach to it.
  help                   Show this help.";

fn main() {
    // All arguments except the program name.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((c, rest)) => (c.as_str(), rest),
        None => ("help", &[][..]),
    };

    let result = match command {
        "status" => status::run(rest.first().is_some_and(|a| a == "--check")),
        "update" => update::run(rest),
        "rollback" => boot::rollback(rest),
        "install-system" => install::run_install(rest),
        "packages" => packages::run(rest),
        "recovery" => recovery::run_recovery(rest),
        "setup" => setup::run_setup(rest),
        "channel" => channel::run(rest),
        "extensions" => ext::run(rest),
        "network" => network::run_network(rest),
        "firewall" => firewall::run_firewall(rest),
        "ssh-server" => ssh::run_ssh_server(rest),
        "snapshots" => snapshots::run_snapshots(rest),
        "health" => health::run_health(rest),
        "report" => report::run_report(rest),
        "encryption" => match rest.is_empty() {
            true if ui::interactive() => {
                util::need_root("encryption").and_then(|_| encryption::screen())
            }
            _ => encryption::run_encryption(rest),
        },
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("xctl: unknown command '{other}'\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    if let Err(e) = result {
        eprintln!("xctl: {e}");
        std::process::exit(1);
    }
}

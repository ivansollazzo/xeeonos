//! xctl - the XeeonOS system tool.
//!
//! Commands: `status`, `update`, `channel`. Rollback and the installer come
//! next. Each command lives in its own module (file) below.

mod channel;
mod oci;
mod status;
mod update;
mod util;
mod version;

/// Fixed paths and names, in one place.
mod config {
    /// Where updates are published: one OCI artifact per version, tagged with
    /// the exact version and with its channel.
    pub const REGISTRY: &str = "ghcr.io/ivansollazzo/xeeonos";
    /// Public keys allowed to sign updates (OpenPGP keyring, as `gpg --export`).
    pub const KEYRING: &str = "/usr/lib/xeeonos/update-keyring.gpg";
    /// Download directory; the sysupdate.d transfer files read from here.
    pub const CACHE: &str = "/var/cache/xctl/update";
    /// The channel chosen by the administrator, if any.
    pub const CHANNEL_FILE: &str = "/etc/xeeonos/channel";
    pub const OS_RELEASE: &str = "/usr/lib/os-release";
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
  channel [<name>]       Show the update channel, or switch to <name>:
                         experimental, or <N>.x-alpha, <N>.x-beta, <N>.x-stable.
      --reset            Go back to the channel this version was built for.
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
        "channel" => channel::run(rest),
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

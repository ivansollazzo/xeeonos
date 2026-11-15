//! Fixed paths and names, in one place.

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
/// The GSettings schemas of the system and of the extensions on,
/// compiled together (GSETTINGS_SCHEMA_DIR, usr/lib/environment.d).
pub const SCHEMAS_RUN: &str = "/run/xeeonos/gsettings";
/// One empty file per extension whose Setup= script has run.
pub const EXTENSIONS_SETUP: &str = "/var/lib/xeeonos/extensions-setup";
/// Present only in the live ISO (mkosi.profiles/live).
pub const LIVE_MARKER: &str = "/usr/lib/xeeonos/live";
/// Kernel command line word of the Recovery boot entry.
pub const RECOVERY_CMDLINE: &str = "xeeonos.recovery";
pub const SYSUPDATE: &str = "/usr/lib/systemd/systemd-sysupdate";
/// GPT partition type of /usr on x86-64 (Discoverable Partitions Specification).
pub const USR_PARTITION_TYPE: &str = "8484680c-9521-48c6-9c11-b0720656f69e";

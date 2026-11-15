//! Everything else xctl does: install, first-boot setup, Recovery,
//! encryption, network, firewall, SSH server, disk health, status, report,
//! packages.

pub mod encryption;
pub mod firewall;
pub mod health;
pub mod install;
pub mod network;
pub mod packages;
pub mod recovery;
pub mod report;
pub mod setup;
pub mod ssh;
pub mod status;

pub use xctl_core::{apps, boot, config, secureboot, snapshots, ui, util, version};
pub use xctl_extensions::ext;
pub use xctl_update::{channel, update};

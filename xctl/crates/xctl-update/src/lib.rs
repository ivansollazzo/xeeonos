//! Updates and channels (`xctl update`, `xctl channel`).

pub mod channel;
pub mod update;

pub use xctl_core::{boot, config, snapshots, util, version};
pub use xctl_extensions::ext;
pub use xctl_fetch::oci;

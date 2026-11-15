//! System extensions (`xctl extensions`).

pub mod ext;

// The paths the module uses, from the crates below.
pub use xctl_core::{config, ui, util, version};
pub use xctl_fetch::manifest as update;

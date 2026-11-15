//! ~/Applications and the desktop: what makes a .xapp an application of the
//! menu, like on macOS.
//!
//!   registry.rs  what xapp sync found (state/apps/<id>)
//!   entry.rs     menu entries (.desktop files), icons, start at login
//!   mime.rs      the file type of .xapp files and who opens them
//!   sync.rs      `xapp sync`, the monitor: brings the menu in line with
//!                ~/Applications (run by a systemd user unit whenever the
//!                folder changes, at login and every 15 minutes)

pub mod entry;
pub mod mime;
pub mod registry;
pub mod sync;

pub use sync::sync;

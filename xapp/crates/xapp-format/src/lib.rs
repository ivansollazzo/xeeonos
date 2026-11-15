//! The .xapp file: one application in one file.
//!
//! ```text
//!   +-------------------------------+  offset 0
//!   | erofs image (read-only file   |  the application's own minimal Arch
//!   | system), with the xapp magic  |  system: /usr, /etc, /xapp/...
//!   | in its unused first bytes     |
//!   +-------------------------------+  image size
//!   | manifest.json                 |  what the application is (manifest.rs)
//!   | signature (64 bytes, Ed25519) |  of the manifest's raw bytes
//!   | icon (PNG or SVG)             |
//!   | trailer (32 bytes)            |  where the pieces are (container.rs)
//!   +-------------------------------+  end of file
//! ```
//!
//! The manifest holds the SHA-256 of the image and of the icon, so the
//! signature of the few bytes of the manifest covers the whole file.

pub mod container;
pub mod manifest;

pub use container::Container;
pub use manifest::{Gpu, Manifest, Permission};

/// The version of the file format this program reads and writes.
pub const FORMAT: u64 = 1;
/// The version of the "contract with the system" (docs/xapp.md): what
/// XeeonOS puts inside every sandbox. An application asks for one.
pub const HOST_CONTRACT: u64 = 1;

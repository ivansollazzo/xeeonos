//! Running an application in its sandbox.
//!
//! An application sees only its own minimal Arch system (the image of its
//! .xapp, mounted read-only), plus what XeeonOS lends it: the "contract with
//! the system" (docs/xapp.md), and what its permissions allow. The pieces:
//!
//!   mount.rs    the image, mounted with erofsfuse (no root needed)
//!   seccomp.rs  the system calls an application may not use
//!   dbus.rs     the filter between the application and the session bus
//!   gpu.rs      the graphics drivers lent by the system
//!   bwrap.rs    the sandbox itself: bubblewrap's command line
//!   launch.rs   putting it together, in a systemd scope of its own

pub mod bwrap;
pub mod dbus;
pub mod gpu;
pub mod launch;
pub mod mount;
pub mod seccomp;

pub use launch::{Launch, run};

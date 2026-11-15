//! Getting XeeonOS files (system updates, extensions), always checked
//! against a signed manifest.

pub mod manifest;
pub mod oci;
pub mod progress;

pub use manifest::Source;

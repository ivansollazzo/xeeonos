//! Getting XeeonOS files (system updates, extensions), always checked
//! against a signed manifest.

pub mod manifest;
pub mod oci;

pub use manifest::Source;

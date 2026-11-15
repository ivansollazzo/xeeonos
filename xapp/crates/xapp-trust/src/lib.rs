//! Trust: is this .xapp really what its author made, and do we know the
//! author? (The Gatekeeper of XeeonOS, docs/xapp.md.)
//!
//!   - the author signs the manifest with an Ed25519 key (keys.rs);
//!   - the manifest holds the SHA-256 of the image, so the signature
//!     covers the whole file: one changed byte and the application is
//!     refused, without a way to force it;
//!   - the first key seen for an application id is remembered ("pinning"):
//!     a file with the same id and another key is refused (a fake update);
//!   - keys, applications and images can be revoked by the system;
//!   - an author whose key the system trusts opens at once; an unknown one
//!     opens only after the user says "Open anyway" (policy.rs).

pub mod keys;
pub mod policy;

pub use policy::{Trust, check, check_image_cached};

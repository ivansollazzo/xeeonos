//! Download files from an OCI artifact on a container registry (ghcr.io),
//! anonymously, using the OCI Distribution API over HTTPS:
//!
//! 1. GET https://<registry>/token?scope=repository:<repo>:pull
//!    -> a short-lived anonymous token for public packages
//! 2. GET https://<registry>/v2/<repo>/manifests/<tag>
//!    -> the manifest: one "layer" per file, with its digest and its file
//!    name in the `org.opencontainers.image.title` annotation (that is how
//!    `oras push` stores files)
//! 3. GET https://<registry>/v2/<repo>/blobs/<digest>
//!    -> the content of one file
//!
//! Integrity is not checked here: the caller verifies the signed SHA256SUMS.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::update::{download_with, is_plain_file_name};
use crate::util::{Result, err};

/// A reference like `ghcr.io/ivansollazzo/xeeonos:experimental`, split up.
#[derive(Debug, PartialEq)]
struct Reference {
    registry: String,
    repository: String,
    tag: String,
}

fn parse_reference(reference: &str) -> Result<Reference> {
    let bad = || {
        err(format!(
            "invalid OCI reference: {reference} (expected registry/name:tag)"
        ))
    };
    let (registry, rest) = reference.split_once('/').ok_or_else(bad)?;
    let (repository, tag) = rest.rsplit_once(':').ok_or_else(bad)?;
    if registry.is_empty() || repository.is_empty() || tag.is_empty() {
        return Err(bad());
    }
    Ok(Reference {
        registry: registry.to_string(),
        repository: repository.to_string(),
        tag: tag.to_string(),
    })
}

/// An opened artifact: we know its files and can download them.
pub struct Artifact {
    base: String,                   // https://<registry>/v2/<repo>
    auth: Vec<String>,              // the Authorization header, if any
    files: HashMap<String, String>, // file name -> blob digest
}

impl Artifact {
    pub fn open(reference: &str) -> Result<Artifact> {
        let r = parse_reference(reference)?;
        let tmp = std::env::temp_dir().join(format!("xctl-oci-{}", std::process::id()));
        fs::create_dir_all(&tmp)?;
        // Run the steps in a closure so the temporary directory is removed
        // whatever happens.
        let result = (|| {
            // 1. Anonymous token (public packages need no account).
            let token_file = tmp.join("token.json");
            download_with(
                &format!(
                    "https://{}/token?scope=repository:{}:pull",
                    r.registry, r.repository
                ),
                &token_file,
                &[],
            )?;
            let token: Value = serde_json::from_str(&fs::read_to_string(&token_file)?)
                .map_err(|e| err(format!("unexpected answer from {}: {e}", r.registry)))?;
            let auth: Vec<String> = token["token"]
                .as_str()
                .map(|t| vec![format!("Authorization: Bearer {t}")])
                .unwrap_or_default();

            // 2. The manifest of the tag.
            let base = format!("https://{}/v2/{}", r.registry, r.repository);
            let manifest_file = tmp.join("manifest.json");
            let mut headers = auth.clone();
            headers.push("Accept: application/vnd.oci.image.manifest.v1+json".to_string());
            download_with(
                &format!("{base}/manifests/{}", r.tag),
                &manifest_file,
                &headers,
            )
            .map_err(|e| err(format!("cannot read {reference}: {e}")))?;
            let manifest = fs::read_to_string(&manifest_file)?;

            Ok(Artifact {
                base,
                auth,
                files: files_in_manifest(&manifest)?,
            })
        })();
        let _ = fs::remove_dir_all(&tmp);
        result
    }

    /// Download the file called `name` into `to`.
    pub fn download(&self, name: &str, to: &Path) -> Result<()> {
        let digest = self
            .files
            .get(name)
            .ok_or_else(|| err(format!("the update has no file named {name}")))?;
        download_with(&format!("{}/blobs/{digest}", self.base), to, &self.auth)
    }
}

/// File name -> digest, from an OCI image manifest (JSON).
fn files_in_manifest(manifest: &str) -> Result<HashMap<String, String>> {
    let json: Value =
        serde_json::from_str(manifest).map_err(|e| err(format!("invalid OCI manifest: {e}")))?;
    let layers = json["layers"]
        .as_array()
        .ok_or_else(|| err("invalid OCI manifest: no layers"))?;

    let mut files = HashMap::new();
    for layer in layers {
        let (Some(name), Some(digest)) = (
            layer["annotations"]["org.opencontainers.image.title"].as_str(),
            layer["digest"].as_str(),
        ) else {
            continue; // a layer that is not a named file: not ours, skip it
        };
        let digest_ok = digest
            .strip_prefix("sha256:")
            .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()));
        if !is_plain_file_name(name) || !digest_ok {
            return Err(err(format!(
                "invalid file in OCI manifest: {name} ({digest})"
            )));
        }
        files.insert(name.to_string(), digest.to_string());
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references() {
        assert_eq!(
            parse_reference("ghcr.io/ivansollazzo/xeeonos:1.x-beta").unwrap(),
            Reference {
                registry: "ghcr.io".into(),
                repository: "ivansollazzo/xeeonos".into(),
                tag: "1.x-beta".into()
            }
        );
        assert!(parse_reference("xeeonos").is_err());
        assert!(parse_reference("ghcr.io/xeeonos").is_err());
    }

    #[test]
    fn manifest() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let m = format!(
            r#"{{"schemaVersion":2,"layers":[
                {{"digest":"{digest}","annotations":{{"org.opencontainers.image.title":"SHA256SUMS"}}}},
                {{"digest":"{digest}"}}
            ]}}"#
        );
        let files = files_in_manifest(&m).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files["SHA256SUMS"], digest);

        let evil = format!(
            r#"{{"layers":[{{"digest":"{digest}","annotations":{{"org.opencontainers.image.title":"../x"}}}}]}}"#
        );
        assert!(files_in_manifest(&evil).is_err());
    }
}

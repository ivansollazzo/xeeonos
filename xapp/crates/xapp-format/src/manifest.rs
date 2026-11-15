//! The manifest: what an application is, what it needs and may ask for.
//! JSON (manifest.json), written by `xapp build`, signed by the author.

use serde_json::{Map, Value, json};
use xapp_core::util::{Result, err, valid_app_id};

/// Where the application's graphics drivers come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gpu {
    /// From the system (Mesa of XeeonOS, always the newest): the default.
    Host,
    /// From the application's own image.
    Bundled,
}

impl Gpu {
    pub fn as_str(self) -> &'static str {
        match self {
            Gpu::Host => "host",
            Gpu::Bundled => "bundled",
        }
    }
}

/// A permission the application may ask for, with the reason shown to the
/// user in the question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permission {
    pub id: String,
    pub reason: String,
}

/// The digest of a piece of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    pub size: u64,
    /// SHA-256, hexadecimal.
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub format: u64,
    pub app_id: String,
    pub name: String,
    /// Translated names: (language, name), e.g. ("it", "Calcolatrice").
    pub names: Vec<(String, String)>,
    pub description: String,
    pub version: String,
    /// The command started in the sandbox, and its fixed arguments.
    pub exec: Vec<String>,
    pub arch: String,
    /// The Arch Linux Archive snapshot of the image (YYYY-MM-DD).
    pub snapshot: String,
    pub host_contract: u64,
    pub gpu: Gpu,
    /// Versions of the C libraries inside the image (glibc, libstdc++ of
    /// gcc-libs): the system's must be at least as new to lend its graphics
    /// drivers.
    pub glibc: String,
    pub libstdcxx: String,
    pub permissions: Vec<Permission>,
    pub mime_types: Vec<String>,
    pub categories: Vec<String>,
    /// The name its windows give themselves (Wayland app id), for the dock.
    pub wm_class: String,
    /// "png" or "svg", and its digest.
    pub icon_type: String,
    pub icon: Digest,
    pub image: Digest,
    /// The author's public key (Ed25519, hexadecimal).
    pub signer: String,
}

fn text(obj: &Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn list(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn digest(obj: &Map<String, Value>, key: &str) -> Result<Digest> {
    let d = obj
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| err(format!("manifest: no {key}")))?;
    let size = d
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| err(format!("manifest: no {key}.size")))?;
    let sha256 = text(d, "sha256");
    if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(err(format!("manifest: bad {key}.sha256")));
    }
    Ok(Digest {
        size,
        sha256: sha256.to_lowercase(),
    })
}

impl Manifest {
    /// Read a manifest (the raw bytes from the file) and check it.
    pub fn parse(bytes: &[u8]) -> Result<Manifest> {
        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| err(format!("manifest is not JSON: {e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| err("manifest is not a JSON object"))?;
        let format = obj.get("format").and_then(Value::as_u64).unwrap_or(0);
        if format != crate::FORMAT {
            return Err(err(format!(
                "format {format} of .xapp files is not supported (this xapp reads format {})",
                crate::FORMAT
            )));
        }
        let gpu = match text(obj, "gpu").as_str() {
            "" | "host" => Gpu::Host,
            "bundled" => Gpu::Bundled,
            other => return Err(err(format!("manifest: unknown gpu \"{other}\""))),
        };
        let names = obj
            .get("names")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let permissions = obj
            .get("permissions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_object)
                    .map(|p| Permission {
                        id: text(p, "id"),
                        reason: text(p, "reason"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let m = Manifest {
            format,
            app_id: text(obj, "app_id"),
            name: text(obj, "name"),
            names,
            description: text(obj, "description"),
            version: text(obj, "version"),
            exec: list(obj, "exec"),
            arch: text(obj, "arch"),
            snapshot: text(obj, "snapshot"),
            host_contract: obj
                .get("host_contract")
                .and_then(Value::as_u64)
                .unwrap_or(1),
            gpu,
            glibc: text(obj, "glibc"),
            libstdcxx: text(obj, "libstdcxx"),
            permissions,
            mime_types: list(obj, "mime_types"),
            categories: list(obj, "categories"),
            wm_class: text(obj, "wm_class"),
            icon_type: text(obj, "icon_type"),
            icon: digest(obj, "icon")?,
            image: digest(obj, "image")?,
            signer: text(obj, "signer").to_lowercase(),
        };
        m.check()?;
        Ok(m)
    }

    /// The rules every manifest must follow.
    pub fn check(&self) -> Result<()> {
        if !valid_app_id(&self.app_id) {
            return Err(err(format!(
                "manifest: \"{}\" is not a valid application id",
                self.app_id
            )));
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            return Err(err("manifest: name and version are required"));
        }
        if self.exec.is_empty() || !self.exec[0].starts_with('/') {
            return Err(err("manifest: exec must start with an absolute path"));
        }
        if self.arch != "x86_64" {
            return Err(err(format!(
                "manifest: architecture {} is not x86_64",
                self.arch
            )));
        }
        if self.host_contract > crate::HOST_CONTRACT {
            return Err(err(format!(
                "the application needs a newer XeeonOS (system contract {}, this one has {})",
                self.host_contract,
                crate::HOST_CONTRACT
            )));
        }
        if !matches!(self.icon_type.as_str(), "png" | "svg") {
            return Err(err("manifest: icon_type must be png or svg"));
        }
        if self.signer.len() != 64 || !self.signer.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(err("manifest: signer is not an Ed25519 public key"));
        }
        for p in &self.permissions {
            if p.id.is_empty() || p.reason.trim().is_empty() {
                return Err(err("manifest: every permission needs an id and a reason"));
            }
        }
        Ok(())
    }

    /// The manifest as JSON text (what `xapp build` signs).
    pub fn to_json(&self) -> Vec<u8> {
        let names: Map<String, Value> = self
            .names
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        let permissions: Vec<Value> = self
            .permissions
            .iter()
            .map(|p| json!({"id": p.id, "reason": p.reason}))
            .collect();
        let value = json!({
            "format": self.format,
            "app_id": self.app_id,
            "name": self.name,
            "names": names,
            "description": self.description,
            "version": self.version,
            "exec": self.exec,
            "arch": self.arch,
            "snapshot": self.snapshot,
            "host_contract": self.host_contract,
            "gpu": self.gpu.as_str(),
            "glibc": self.glibc,
            "libstdcxx": self.libstdcxx,
            "permissions": permissions,
            "mime_types": self.mime_types,
            "categories": self.categories,
            "wm_class": self.wm_class,
            "icon_type": self.icon_type,
            "icon": {"size": self.icon.size, "sha256": self.icon.sha256},
            "image": {"size": self.image.size, "sha256": self.image.sha256},
            "signer": self.signer,
        });
        let mut out = serde_json::to_vec_pretty(&value).unwrap_or_default();
        out.push(b'\n');
        out
    }

    /// The name in a language ("it_IT.UTF-8" -> "it"), else the default.
    pub fn name_for(&self, lang: &str) -> &str {
        let short = lang.split(['_', '.', '@']).next().unwrap_or("");
        self.names
            .iter()
            .find(|(l, _)| l == lang || l == short)
            .map(|(_, n)| n.as_str())
            .unwrap_or(&self.name)
    }

    /// The name in the user's language ($LANG).
    pub fn display_name(&self) -> &str {
        self.name_for(&std::env::var("LANG").unwrap_or_default())
    }

    /// Does the application declare this permission? (Then its reason.)
    pub fn declares(&self, id: &str) -> Option<&Permission> {
        self.permissions.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample() -> Manifest {
        Manifest {
            format: 1,
            app_id: "org.xeeonos.Hello".into(),
            name: "Hello".into(),
            names: vec![("it".into(), "Ciao".into())],
            description: "A test".into(),
            version: "1.0".into(),
            exec: vec!["/usr/bin/hello".into(), "--flag".into()],
            arch: "x86_64".into(),
            snapshot: "2026-11-13".into(),
            host_contract: 1,
            gpu: Gpu::Host,
            glibc: "2.42".into(),
            libstdcxx: "15.2.1".into(),
            permissions: vec![Permission {
                id: "documents".into(),
                reason: "To open your files".into(),
            }],
            mime_types: vec!["text/plain".into()],
            categories: vec!["Utility".into()],
            wm_class: "hello".into(),
            icon_type: "png".into(),
            icon: Digest {
                size: 3,
                sha256: "a".repeat(64),
            },
            image: Digest {
                size: 4096,
                sha256: "b".repeat(64),
            },
            signer: "c".repeat(64),
        }
    }

    #[test]
    fn round_trip() {
        let m = sample();
        let parsed = Manifest::parse(&m.to_json()).unwrap();
        assert_eq!(parsed, m);
        assert_eq!(parsed.name_for("it_IT.UTF-8"), "Ciao");
        assert_eq!(parsed.name_for("de_DE.UTF-8"), "Hello");
        assert!(parsed.declares("documents").is_some());
        assert!(parsed.declares("camera").is_none());
    }

    #[test]
    fn rejects() {
        let mut m = sample();
        m.app_id = "hello".into();
        assert!(Manifest::parse(&m.to_json()).is_err());
        let mut m = sample();
        m.exec = vec!["hello".into()];
        assert!(Manifest::parse(&m.to_json()).is_err());
        let mut m = sample();
        m.format = 2;
        assert!(Manifest::parse(&m.to_json()).is_err());
        let mut m = sample();
        m.host_contract = 99;
        assert!(Manifest::parse(&m.to_json()).is_err());
        assert!(Manifest::parse(b"not json").is_err());
    }
}

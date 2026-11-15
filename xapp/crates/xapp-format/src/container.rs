//! Reading and writing the pieces of a .xapp file (see lib.rs).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use xapp_core::util::{Result, err, hex};

use crate::manifest::Manifest;

/// Written at offset 0 of the image, in the first 1024 bytes that erofs
/// leaves unused: it lets the desktop recognize a .xapp from its content
/// (application/x-xapp), whatever its name.
pub const MAGIC: &[u8; 8] = b"\x7fXAPP1\n\0";
/// The last 32 bytes of the file start with this.
pub const TRAILER_MAGIC: &[u8; 8] = b"XAPPEND1";
pub const TRAILER_LEN: u64 = 32;
pub const SIGNATURE_LEN: usize = 64;
const MAX_MANIFEST: u64 = 1 << 20;
const MAX_ICON: u64 = 4 << 20;

/// A .xapp file, opened: its manifest, signature and icon, read from the end
/// of the file (the image itself is used in place, mounted).
pub struct Container {
    pub path: PathBuf,
    pub image_size: u64,
    pub manifest_bytes: Vec<u8>,
    pub signature: [u8; SIGNATURE_LEN],
    pub icon: Vec<u8>,
    pub manifest: Manifest,
}

fn le64(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap_or([0; 8]))
}

fn le32(b: &[u8]) -> u64 {
    u32::from_le_bytes(b.try_into().unwrap_or([0; 4])) as u64
}

impl Container {
    /// Open a .xapp file and read its manifest. This checks the layout, not
    /// the signature nor the image (see `xapp_trust` and `check_image`).
    pub fn open(path: &Path) -> Result<Container> {
        let not_xapp = || {
            err(format!(
                "{} is not a .xapp application (or it is incomplete)",
                path.display()
            ))
        };
        let mut f =
            File::open(path).map_err(|e| err(format!("cannot open {}: {e}", path.display())))?;
        let len = f.metadata()?.len();
        if len < 4096 + TRAILER_LEN {
            return Err(not_xapp());
        }
        let mut magic = [0u8; 8];
        f.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(not_xapp());
        }
        let mut trailer = [0u8; TRAILER_LEN as usize];
        f.seek(SeekFrom::End(-(TRAILER_LEN as i64)))?;
        f.read_exact(&mut trailer)?;
        if &trailer[0..8] != TRAILER_MAGIC {
            return Err(not_xapp());
        }
        let image_size = le64(&trailer[8..16]);
        let manifest_len = le32(&trailer[16..20]);
        let icon_len = le32(&trailer[20..24]);
        if manifest_len > MAX_MANIFEST
            || icon_len > MAX_ICON
            || image_size.checked_add(manifest_len + SIGNATURE_LEN as u64 + icon_len + TRAILER_LEN)
                != Some(len)
        {
            return Err(not_xapp());
        }
        f.seek(SeekFrom::Start(image_size))?;
        let mut manifest_bytes = vec![0u8; manifest_len as usize];
        f.read_exact(&mut manifest_bytes)?;
        let mut signature = [0u8; SIGNATURE_LEN];
        f.read_exact(&mut signature)?;
        let mut icon = vec![0u8; icon_len as usize];
        f.read_exact(&mut icon)?;
        let manifest = Manifest::parse(&manifest_bytes)?;
        Ok(Container {
            path: path.to_path_buf(),
            image_size,
            manifest_bytes,
            signature,
            icon,
            manifest,
        })
    }

    /// Check that the image and the icon are the ones the manifest names.
    /// Reads the whole image (about a second for a big application).
    pub fn check_image(&self) -> Result<()> {
        let m = &self.manifest;
        if m.image.size != self.image_size {
            return Err(err(
                "the image does not have the size its manifest says: the file was changed",
            ));
        }
        if hash_file_prefix(&self.path, self.image_size)? != m.image.sha256 {
            return Err(err(
                "the image does not match its manifest: the file was changed",
            ));
        }
        if m.icon.size != self.icon.len() as u64
            || hex(&Sha256::digest(&self.icon)) != m.icon.sha256
        {
            return Err(err(
                "the icon does not match its manifest: the file was changed",
            ));
        }
        Ok(())
    }
}

/// SHA-256 (hexadecimal) of the first `size` bytes of a file.
pub fn hash_file_prefix(path: &Path, size: u64) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = size;
    while left > 0 {
        let n = (buf.len() as u64).min(left) as usize;
        f.read_exact(&mut buf[..n])?;
        hasher.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(hex(&hasher.finalize()))
}

/// SHA-256 (hexadecimal) of some bytes.
pub fn hash_bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Put the magic at the start of a freshly made erofs image. Its first 1024
/// bytes are unused by erofs and must still be zero.
pub fn stamp_image(image: &Path) -> Result<()> {
    let mut f = fs::OpenOptions::new().read(true).write(true).open(image)?;
    let mut head = [0u8; 8];
    f.read_exact(&mut head)?;
    if head != [0u8; 8] && &head != MAGIC {
        return Err(err(
            "the image does not start with empty bytes: not an erofs image?",
        ));
    }
    f.seek(SeekFrom::Start(0))?;
    f.write_all(MAGIC)?;
    Ok(())
}

/// Write a .xapp: the (stamped) image, then manifest, signature, icon and
/// trailer.
pub fn write(
    out: &Path,
    image: &Path,
    manifest: &[u8],
    signature: &[u8; SIGNATURE_LEN],
    icon: &[u8],
) -> Result<()> {
    let image_size = fs::metadata(image)?.len();
    let tmp = out.with_extension("xapp.part");
    {
        let mut dst = File::create(&tmp)?;
        let mut src = File::open(image)?;
        std::io::copy(&mut src, &mut dst)?;
        dst.write_all(manifest)?;
        dst.write_all(signature)?;
        dst.write_all(icon)?;
        let mut trailer = Vec::with_capacity(TRAILER_LEN as usize);
        trailer.extend_from_slice(TRAILER_MAGIC);
        trailer.extend_from_slice(&image_size.to_le_bytes());
        trailer.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        trailer.extend_from_slice(&(icon.len() as u32).to_le_bytes());
        trailer.resize(TRAILER_LEN as usize, 0);
        dst.write_all(&trailer)?;
        dst.sync_all()?;
    }
    fs::rename(&tmp, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::sample;

    #[test]
    fn write_open_check() {
        let dir = std::env::temp_dir().join(format!("xapp-format-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let image = dir.join("image.erofs");
        let mut data = vec![0u8; 8192];
        data[1024] = 0xe2; // anything
        fs::write(&image, &data).unwrap();
        stamp_image(&image).unwrap();
        let icon = b"PNG".to_vec();
        let mut m = sample();
        m.image.size = 8192;
        m.image.sha256 = hash_file_prefix(&image, 8192).unwrap();
        m.icon.size = 3;
        m.icon.sha256 = hash_bytes(&icon);
        let out = dir.join("hello.xapp");
        write(&out, &image, &m.to_json(), &[7u8; 64], &icon).unwrap();

        let c = Container::open(&out).unwrap();
        assert_eq!(c.manifest, m);
        assert_eq!(c.signature, [7u8; 64]);
        assert_eq!(c.icon, icon);
        c.check_image().unwrap();

        // One byte changed in the image: refused.
        let mut bytes = fs::read(&out).unwrap();
        bytes[5000] ^= 1;
        fs::write(&out, &bytes).unwrap();
        assert!(Container::open(&out).unwrap().check_image().is_err());
        // Cut short (still copying): not a .xapp.
        fs::write(&out, &bytes[..bytes.len() - 10]).unwrap();
        assert!(Container::open(&out).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! Deterministic FNV-1a helpers shared by review feedback records.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Stable FNV-1a digest over ordered key/value pairs.
pub(crate) fn fingerprint(parts: &[(&str, String)]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (key, value) in parts {
        for bytes in [key.as_bytes(), b"=", value.as_bytes(), b"\n"] {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    format!("{hash:016x}")
}

/// FNV-1a digest of a file's bytes, used to register holdout fixtures.
pub fn checksum_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("cannot read {}", path.display()))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

use anyhow::{ensure, Result};

/// Same algorithm as Java's UUID.nameUUIDFromBytes("OfflinePlayer:<name>").
/// Returned without hyphens, which is what the launch arguments expect.
pub fn offline_uuid(name: &str) -> String {
    let mut d = md5::compute(format!("OfflinePlayer:{name}").as_bytes()).0;
    d[6] = (d[6] & 0x0f) | 0x30; // version 3
    d[8] = (d[8] & 0x3f) | 0x80; // IETF variant
    hex::encode(d)
}

pub fn validate_username(name: &str) -> Result<()> {
    ensure!(
        (3..=16).contains(&name.len())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "username must be 3-16 characters: letters, digits or underscore"
    );
    Ok(())
}

// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Anonymous release/updater acceptance: download and verify, never install or open a profile.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mailsolutions_net::{update, update_download};
    let dir = std::env::args_os().nth(1).map(std::path::PathBuf::from).ok_or("usage: release_verify <empty-output-directory>")?;
    if dir.exists() {
        return Err("output directory must not exist; preserve previous verification downloads".into());
    }
    let release = update::latest(false)?;
    if release.version.to_string() != env!("CARGO_PKG_VERSION") {
        return Err("latest published version does not match this source".into());
    }
    std::fs::create_dir_all(&dir)?;
    let mut assets = Vec::new();
    for suffix in ["windows-x64.msi", "windows-x64-portable.zip"] {
        let name = format!("mailsolutions-{}-{suffix}", release.version);
        update_download::download(&release, &name, &dir).map_err(|e| e.to_string())?;
        let hash = update_download::sha256(&dir.join(&name)).ok_or("downloaded file cannot be hashed")?;
        let asset = release.assets.iter().find(|a| a.name == name).ok_or("release asset missing")?;
        assets.push(serde_json::json!({"name":name,"bytes":std::fs::metadata(dir.join(&name))?.len(),"sha256":hash.iter().map(|b|format!("{b:02x}")).collect::<String>(),"url":asset.url}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"repository":update::REPOSITORY,"version":release.version.to_string(),"url":release.page,"assets":assets,"checksumSignatureVerified":true,"installed":false})
        )?
    );
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}

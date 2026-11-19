// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Offline profile cutover. Originals are retained; no account or credential store is opened.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};

const RECEIPT: &str = "rename-receipt.json";
const MAX_FILES: usize = 100_000;

#[derive(Serialize, Deserialize)]
struct Receipt {
    version: u32,
    source: PathBuf,
    files: BTreeMap<PathBuf, String>,
}

fn failure(why: impl std::fmt::Display) -> String {
    format!(
        "MailSolutions profile migration stopped: {why}. Originals were retained. See docs/rename-migration.md; do not merge or delete either profile."
    )
}

fn exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(failure(e)),
    }
}

fn ordinary(path: &Path) -> Result<fs::Metadata, String> {
    let m = fs::symlink_metadata(path).map_err(failure)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err(failure("a profile entry is a reparse point"));
        }
    }
    if m.file_type().is_symlink() || (!m.is_file() && !m.is_dir()) {
        return Err(failure("a profile entry is not an ordinary file or directory"));
    }
    Ok(m)
}

fn inventory(base: &Path, dir: &Path, depth: usize, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 64 || files.len() >= MAX_FILES {
        return Err(failure("profile exceeds migration limits"));
    }
    ordinary(dir)?;
    for entry in fs::read_dir(dir).map_err(failure)? {
        let path = entry.map_err(failure)?.path();
        if ordinary(&path)?.is_dir() {
            inventory(base, &path, depth + 1, files)?;
        } else {
            if files.len() >= MAX_FILES {
                return Err(failure("profile exceeds migration limits"));
            }
            files.push(path.strip_prefix(base).map_err(failure)?.to_path_buf());
        }
    }
    files.sort();
    Ok(())
}

fn held_read(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ alone: refuse existing writers and deny writes/deletes while held.
        options.share_mode(1);
    }
    options.open(path).map_err(|_| failure("a profile file is busy or inaccessible; close its owner in an approved maintenance window and retry"))
}

fn digest(file: &mut File, mut copy: Option<&mut File>) -> Result<String, String> {
    let mut hash = Context::new(&SHA256);
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(failure)?;
        if n == 0 {
            break;
        }
        let bytes = buffer.get(..n).ok_or_else(|| failure("invalid read size"))?;
        hash.update(bytes);
        if let Some(out) = copy.as_mut() {
            out.write_all(bytes).map_err(failure)?;
        }
    }
    Ok(hash.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect())
}

fn remap_recent(settings: &mut serde_json::Value, source: &Path, destination: &Path) {
    // Only the known path list, never arbitrary message/settings text or custom mailbox paths.
    if let Some(recent) = settings.get_mut("recent").and_then(serde_json::Value::as_array_mut) {
        for value in recent {
            if let Some(path) = value.as_str()
                && let Ok(relative) = Path::new(path).strip_prefix(source)
            {
                *value = serde_json::Value::String(destination.join(relative).to_string_lossy().into_owned());
            }
        }
    }
}

/// Called before update state, preferences, keys or logs are opened. Never for --sample/--version.
pub fn startup() -> Result<(), String> {
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("APPDATA") {
        let base = PathBuf::from(base);
        prepare(&base.join("MailCraft"), &base.join("MailSolutions"))?;
    }
    Ok(())
}

/// Explicit paths make the complete migration testable with synthetic, isolated profiles.
pub fn prepare(source: &Path, destination: &Path) -> Result<(), String> {
    if source == destination || source.starts_with(destination) || destination.starts_with(source) {
        return Err(failure("source and destination overlap"));
    }
    if !exists(source)? {
        return Ok(());
    }
    if !ordinary(source)?.is_dir() {
        return Err(failure("source is not a directory"));
    }
    let stage = destination.with_extension("migration-staging");
    let lock_path = destination.with_extension("migration-lock");
    if exists(&stage)? || exists(&lock_path)? {
        return Err(failure("an interrupted or concurrent migration needs review"));
    }
    let mut paths = Vec::new();
    inventory(source, source, 0, &mut paths)?;
    let mut held = Vec::new();
    // Hold every original against writes/deletion for the entire transaction, not one at a time.
    for relative in &paths {
        held.push((relative.clone(), held_read(&source.join(relative))?));
    }
    let mut hashes = BTreeMap::new();
    if exists(destination)? {
        ordinary(destination)?;
        let receipt_path = destination.join(RECEIPT);
        ordinary(&receipt_path)?;
        if fs::metadata(&receipt_path).map_err(failure)?.len() > 32 * 1024 * 1024 {
            return Err(failure("migration receipt exceeds its size limit"));
        }
        let receipt: Receipt =
            serde_json::from_reader(held_read(&receipt_path)?).map_err(|_| failure("both profiles exist without a valid receipt"))?;
        for (relative, file) in &mut held {
            hashes.insert(relative.clone(), digest(file, None)?);
        }
        if receipt.version != 1 || receipt.source != source || receipt.files != hashes {
            return Err(failure("the retained original changed or the two profiles conflict"));
        }
        return Ok(());
    }
    let lock = OpenOptions::new().write(true).create_new(true).open(&lock_path).map_err(failure)?;
    // Any failure after this point deliberately leaves the stage/lock for recovery, never guesses.
    mailsolutions_engine::store::create_private_dir_all(&stage).map_err(failure)?;
    for (relative, file) in &mut held {
        let to = stage.join(&*relative);
        if let Some(parent) = to.parent() {
            mailsolutions_engine::store::create_private_dir_all(parent).map_err(failure)?;
        }
        let mut out = OpenOptions::new().write(true).create_new(true).open(&to).map_err(failure)?;
        let hash = digest(file, Some(&mut out))?;
        out.sync_all().map_err(failure)?;
        drop(out);
        if digest(&mut File::open(&to).map_err(failure)?, None)? != hash {
            return Err(failure("copied bytes do not match their original"));
        }
        hashes.insert(relative.clone(), hash);
    }
    let prefs = stage.join("ui.json");
    if prefs.exists() {
        if fs::metadata(&prefs).map_err(failure)?.len() > 16 * 1024 * 1024 {
            return Err(failure("settings exceed migration limits"));
        }
        let mut json: serde_json::Value =
            serde_json::from_reader(File::open(&prefs).map_err(failure)?).map_err(|_| failure("settings JSON is invalid"))?;
        let _: mailsolutions_ui_egui::UiState = serde_json::from_value(json.clone()).map_err(|_| failure("settings cannot be read"))?;
        remap_recent(&mut json, source, destination);
        if let Some(options) = json.get_mut("options").and_then(serde_json::Value::as_object_mut)
            && let Some(old) = options.remove("contactcraftContacts")
        {
            options.insert("contactsolutionsContacts".into(), old);
        }
        fs::write(&prefs, serde_json::to_vec_pretty(&json).map_err(failure)?).map_err(failure)?;
    }
    let mailbox = stage.join("mailbox.mailcraft");
    if mailbox.exists() {
        let opened = mailsolutions_engine::store::open(&mailbox).map_err(|_| failure("mailbox validation failed"))?;
        if opened.unavailable != 0 || opened.unfinished != 0 || opened.lost_saves != 0 || opened.lost_records != 0 {
            return Err(failure("mailbox needs repair; original was not changed"));
        }
    }
    // Old pending installers must never become this product's pending update.
    if stage.join("updates").exists() {
        fs::rename(stage.join("updates"), stage.join("updates.pre-rename")).map_err(failure)?;
    }
    let mut after = Vec::new();
    inventory(source, source, 0, &mut after)?;
    if after != paths {
        return Err(failure("the source profile changed during migration"));
    }
    let receipt = Receipt { version: 1, source: source.to_path_buf(), files: hashes };
    let mut out = OpenOptions::new().write(true).create_new(true).open(stage.join(RECEIPT)).map_err(failure)?;
    out.write_all(&serde_json::to_vec_pretty(&receipt).map_err(failure)?).map_err(failure)?;
    out.sync_all().map_err(failure)?;
    drop(out);
    fs::rename(&stage, destination).map_err(failure)?;
    drop(lock);
    fs::remove_file(lock_path).map_err(failure)?;
    Ok(())
}

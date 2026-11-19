// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synthetic profiles only. No environment overrides, live accounts, keyrings or installers.

use super::profile_migration::prepare;
use std::fs;
use tempfile::TempDir;

#[test]
fn old_profile_keeps_ids_settings_blobs_and_recovery_copy() {
    let temp = TempDir::new().unwrap();
    let old = temp.path().join("MailCraft");
    let new = temp.path().join("MailSolutions");
    fs::create_dir(&old).unwrap();
    let mailbox = mailsolutions_engine::sample::mailbox(1_800_000_000, 0);
    mailsolutions_engine::store::save(&old.join("mailbox.mailcraft"), &mailbox).unwrap();
    let original = fs::read(old.join("mailbox.mailcraft")).unwrap();
    assert!(original.starts_with(b"MailCraft index\n"));
    let prefs = serde_json::json!({"theme":"dark", "options":{"contactcraftContacts":true}, "recent":[old.join("mailbox.mailcraft"), temp.path().join("custom.mailcraft")], "futureSetting":{"kept":42}});
    fs::write(old.join("ui.json"), serde_json::to_vec(&prefs).unwrap()).unwrap();
    fs::create_dir(old.join("updates")).unwrap();
    fs::write(old.join("updates/pending.json"), b"synthetic old update").unwrap();
    prepare(&old, &new).unwrap();
    assert_eq!(fs::read(old.join("mailbox.mailcraft")).unwrap(), original);
    let opened = mailsolutions_engine::store::open(&new.join("mailbox.mailcraft")).unwrap();
    assert_eq!(opened.mailbox.accounts.iter().map(|a| a.id).collect::<Vec<_>>(), mailbox.accounts.iter().map(|a| a.id).collect::<Vec<_>>());
    assert_eq!(opened.mailbox.messages.keys().collect::<Vec<_>>(), mailbox.messages.keys().collect::<Vec<_>>());
    assert_eq!(opened.unavailable, 0);
    let copied: serde_json::Value = serde_json::from_slice(&fs::read(new.join("ui.json")).unwrap()).unwrap();
    assert_eq!(copied["futureSetting"], prefs["futureSetting"]);
    assert_eq!(copied["recent"][0], serde_json::json!(new.join("mailbox.mailcraft")));
    assert_eq!(copied["recent"][1], prefs["recent"][1]);
    let state: mailsolutions_ui_egui::UiState = serde_json::from_value(copied).unwrap();
    assert!(state.options.contactsolutions_contacts);
    super::write_prefs(&new.join("ui.json"), &state).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(new.join("ui.json")).unwrap()).unwrap();
    assert_eq!(saved["futureSetting"], prefs["futureSetting"]);
    assert!(serde_json::from_value::<mailsolutions_ui_egui::UiState>(saved).unwrap().options.contactsolutions_contacts);
    assert!(new.join("updates.pre-rename/pending.json").is_file());
    assert!(!new.join("updates").exists());
    prepare(&old, &new).unwrap();
    fs::write(old.join("new-file"), b"changed retained source").unwrap();
    assert!(prepare(&old, &new).is_err());
}

#[test]
fn conflicts_invalid_settings_and_interrupted_stages_never_overwrite() {
    for case in ["both", "invalid", "stage", "lock", "overlap"] {
        let temp = TempDir::new().unwrap();
        let old = temp.path().join("MailCraft");
        let new = temp.path().join("MailSolutions");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("ui.json"), if case == "invalid" { b"{".as_slice() } else { b"{}".as_slice() }).unwrap();
        let before = fs::read(old.join("ui.json")).unwrap();
        match case {
            "both" => fs::create_dir(&new).unwrap(),
            "stage" => fs::create_dir(new.with_extension("migration-staging")).unwrap(),
            "lock" => fs::write(new.with_extension("migration-lock"), b"held").unwrap(),
            "overlap" => {
                assert!(prepare(&old, &old.join("nested")).is_err());
                continue;
            }
            _ => {}
        }
        assert!(prepare(&old, &new).is_err(), "{case}");
        assert_eq!(fs::read(old.join("ui.json")).unwrap(), before);
        assert!(!new.join("ui.json").exists());
    }
}

#[cfg(windows)]
#[test]
fn a_live_writer_blocks_migration_without_touching_its_file() {
    use std::fs::OpenOptions;
    let temp = TempDir::new().unwrap();
    let old = temp.path().join("MailCraft");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("ui.json"), b"{}").unwrap();
    let _held = OpenOptions::new().write(true).open(old.join("ui.json")).unwrap();
    let new = temp.path().join("MailSolutions");
    assert!(prepare(&old, &new).is_err());
    assert!(!new.exists());
    assert!(!new.with_extension("migration-staging").exists());
}

#[test]
fn legacy_mailbox_fixture_opens_after_profile_copy() {
    let temp = TempDir::new().unwrap();
    let old = temp.path().join("MailCraft");
    let new = temp.path().join("MailSolutions");
    fs::create_dir(&old).unwrap();
    let bytes = include_bytes!("../../../crates/engine/testdata/mailbox-0.2.1.mailcraft");
    fs::write(old.join("mailbox.mailcraft"), bytes).unwrap();
    prepare(&old, &new).unwrap();
    let opened = mailsolutions_engine::store::open(&new.join("mailbox.mailcraft")).unwrap();
    assert!(!opened.mailbox.accounts.is_empty());
    assert_eq!(fs::read(old.join("mailbox.mailcraft")).unwrap(), bytes);
}

// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mailcraft_engine::Session;
use mailcraft_model::{Contact, Labeled, Mailbox};
use serde_json::json;

use super::*;
use crate::{Services, UiState};

#[test]
fn choosing_contactcraft_uses_its_snapshot_and_keeps_local_contacts_separate() {
    let mut mailbox = Mailbox::with_local_account("Owner", "owner@example.org");
    mailbox.add_contact(Contact { first: "Local".into(), emails: vec![Labeled::new("home", "local@example.org")], ..Default::default() });
    let opened = Arc::new(AtomicUsize::new(0));
    let count = opened.clone();
    let services = Services {
        contactcraft_load: Some(Box::new(|revision| {
            let (tx, rx) = std::sync::mpsc::channel();
            let answer = if revision == 4 {
                None
            } else {
                Some(SharedContacts {
                    revision: 4,
                    contacts: vec![Contact {
                        contactcraft_id: "shared-id".into(),
                        first: "Shared".into(),
                        emails: vec![Labeled::new("work", "shared@example.org")],
                        ..Default::default()
                    }],
                })
            };
            let _ = tx.send(Ok(answer));
            rx
        })),
        contactcraft_action: Some(Box::new(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = tx.send(Ok(()));
            rx
        })),
        ..Default::default()
    };
    let mut app = MailApp::new(Session::new(mailbox), services);
    app.apply_prefs(UiState::default());
    let list = app.execute("people.list", json!({})).unwrap();
    assert_eq!(list["contacts"][0]["name"], "Local");

    app.execute("ui.contactManager", json!({"value": "contactcraft"})).unwrap();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    tick(&mut app, &ctx);
    let list = app.execute("people.list", json!({})).unwrap();
    assert_eq!(list["contacts"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["contacts"][0]["name"], "Shared");
    let suggestions = mailcraft_engine::cmd::compose::suggest(&app.session, "sha", 5);
    assert!(suggestions.iter().any(|s| s.email == "shared@example.org"));
    assert_eq!(app.run("people.new", json!({})).unwrap()["pending"], "contactcraft");
    assert_eq!(opened.load(Ordering::Relaxed), 1);

    app.execute("ui.contactManager", json!({"value": "included"})).unwrap();
    let list = app.execute("people.list", json!({})).unwrap();
    assert_eq!(list["contacts"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["contacts"][0]["name"], "Local");
    assert!(app.session.mailbox.contacts.values().all(|c| c.contactcraft_id.is_empty()));
}

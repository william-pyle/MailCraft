// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! An optional live view of ContactCraft's shared contacts. The desktop host owns the store
//! and does every disk access on a background thread; the window receives snapshots only.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, TryRecvError};

use mailcraft_model::{Contact, Id};
use serde_json::{Value, json};

use crate::MailApp;

/// A changed ContactCraft revision and its contacts, converted by the desktop host.
pub struct SharedContacts {
    pub revision: i64,
    pub contacts: Vec<Contact>,
}

/// Work the desktop host performs against ContactCraft's store or editor.
pub enum SharedAction {
    New,
    Edit(String),
    EditEmail(String),
    Delete(String),
    Favorite { id: String, value: bool },
    Category { id: String, name: String, value: bool },
}

pub(crate) struct State {
    ids: BTreeMap<String, Id>,
    pub revision: i64,
    opening: u64,
    next_check: i64,
    load: Option<Receiver<Result<Option<SharedContacts>, String>>>,
    actions: Vec<Receiver<Result<(), String>>>,
}

impl Default for State {
    fn default() -> Self {
        Self { ids: BTreeMap::new(), revision: -1, opening: 0, next_check: i64::MIN, load: None, actions: Vec::new() }
    }
}

fn active(app: &MailApp) -> bool {
    app.session.prefs.contactcraft_contacts && app.services.contactcraft_load.is_some()
}

fn forget(app: &mut MailApp) {
    let selected = app.session.sel.contact;
    app.session.mailbox.contacts.retain(|_, c| c.contactcraft_id.is_empty());
    if selected.is_some_and(|id| app.session.mailbox.contact(id).is_none()) {
        app.session.sel.contact = None;
    }
    app.people_picked.clear();
    app.contactcraft.ids.clear();
    app.contactcraft.revision = -1;
    app.contactcraft.load = None;
    app.contactcraft.next_check = i64::MIN;
    app.session.view.people_account = mailcraft_model::AccountChoice::All;
    app.session.transient_contacts_changed();
}

fn install(app: &mut MailApp, snapshot: SharedContacts) {
    let selected =
        app.session.sel.contact.and_then(|id| app.session.mailbox.contact(id)).map(|c| c.contactcraft_id.clone()).filter(|id| !id.is_empty());
    app.session.mailbox.contacts.retain(|_, c| c.contactcraft_id.is_empty());
    let mut ids = BTreeMap::new();
    for mut c in snapshot.contacts.into_iter().take(20_000) {
        if c.contactcraft_id.is_empty() {
            continue;
        }
        let source = c.contactcraft_id.clone();
        if let Some(old) = app.contactcraft.ids.get(&source) {
            c.id = *old;
        }
        let id = app.session.mailbox.add_contact(c);
        ids.insert(source, id);
    }
    if let Some(source) = selected {
        app.session.sel.contact = ids.get(&source).copied();
    }
    app.contactcraft.ids = ids;
    app.contactcraft.revision = snapshot.revision;
    app.people_picked.clear();
    app.session.transient_contacts_changed();
}

/// Poll without blocking the frame. ContactCraft may be closed; its WAL store remains available.
pub(crate) fn tick(app: &mut MailApp, ctx: &egui::Context) {
    if !active(app) {
        return;
    }
    if app.contactcraft.opening != app.session.openings() {
        app.contactcraft.opening = app.session.openings();
        app.contactcraft.revision = -1;
        app.contactcraft.ids.clear();
        app.contactcraft.load = None;
        app.contactcraft.next_check = i64::MIN;
    }
    let mut changed = false;
    let mut errors = Vec::new();
    app.contactcraft.actions.retain_mut(|rx| match rx.try_recv() {
        Ok(Ok(())) => {
            changed = true;
            false
        }
        Ok(Err(e)) => {
            errors.push(e);
            changed = true;
            false
        }
        Err(TryRecvError::Disconnected) => {
            errors.push("ContactCraft stopped before completing the change.".to_string());
            changed = true;
            false
        }
        Err(TryRecvError::Empty) => true,
    });
    for error in errors {
        app.status(error);
    }
    if changed {
        app.contactcraft.revision = -1;
        app.contactcraft.load = None;
        app.contactcraft.next_check = i64::MIN;
    }
    if let Some(rx) = &app.contactcraft.load {
        match rx.try_recv() {
            Ok(Ok(Some(snapshot))) => {
                app.contactcraft.load = None;
                install(app, snapshot);
            }
            Ok(Ok(None)) => {
                app.contactcraft.load = None;
            }
            Ok(Err(e)) => {
                app.contactcraft.load = None;
                app.status(format!("ContactCraft contacts couldn't be read: {e}"));
                app.contactcraft.next_check = app.session.now().saturating_add(15);
            }
            Err(TryRecvError::Disconnected) => {
                app.contactcraft.load = None;
                app.contactcraft.next_check = app.session.now().saturating_add(15);
            }
            Err(TryRecvError::Empty) => {}
        }
    }
    let now = app.session.now();
    if app.contactcraft.load.is_none()
        && now >= app.contactcraft.next_check
        && let Some(load) = &app.services.contactcraft_load
    {
        app.contactcraft.load = Some(load(app.contactcraft.revision));
        app.contactcraft.next_check = now.saturating_add(2);
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(2));
}

fn submit(app: &mut MailApp, action: SharedAction) -> Result<Value, String> {
    let Some(host) = &app.services.contactcraft_action else {
        return Err("ContactCraft isn't installed on this computer.".into());
    };
    app.contactcraft.actions.push(host(action));
    Ok(json!({"pending": "contactcraft"}))
}

/// An owner's New/Edit/Add Sender action opens ContactCraft's full editor.
pub(crate) fn by_hand(app: &mut MailApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !active(app) {
        return None;
    }
    let action = match id {
        "people.new" if params.as_object().is_none_or(serde_json::Map::is_empty) => SharedAction::New,
        "people.edit" if params.as_object().is_none_or(serde_json::Map::is_empty) => {
            let selected = app.session.sel.contact.and_then(|id| app.session.mailbox.contact(id));
            let Some(source) = selected.map(|c| c.contactcraft_id.clone()).filter(|s| !s.is_empty()) else {
                return Some(Err("Select a ContactCraft contact first.".into()));
            };
            SharedAction::Edit(source)
        }
        "people.fromMessage" => {
            let mid = params.get("id").and_then(Value::as_u64).or(app.session.sel.focus());
            let email = mid.and_then(|id| app.session.mailbox.message(id)).map(|m| m.from.email.clone()).filter(|e| !e.trim().is_empty());
            let Some(email) = email else {
                return Some(Err("Select a message with an email sender first.".into()));
            };
            SharedAction::EditEmail(email)
        }
        "people.merge" => return Some(Err("Merge contacts in ContactCraft while it is the contacts manager.".into())),
        _ => return None,
    };
    Some(submit(app, action))
}

/// Programmatic changes never silently write the included mailbox while ContactCraft is chosen.
pub(crate) fn execute(app: &mut MailApp, command: &str, params: &Value) -> Option<Result<Value, String>> {
    if !active(app) {
        return None;
    }
    if matches!(command, "people.new" | "people.edit" | "people.fromMessage" | "people.merge") {
        return Some(Err("ContactCraft is the contacts manager. Open its editor in MailCraft to add or edit contacts.".into()));
    }
    if !matches!(command, "people.delete" | "people.favorite" | "people.categorize") {
        return None;
    }
    let target = params.get("id").and_then(Value::as_u64).or(app.session.sel.contact);
    let Some(id) = target else {
        return Some(Err("Select a contact first.".into()));
    };
    let Some(c) = app.session.mailbox.contact(id).cloned() else {
        return Some(Err("No contact with that ID.".into()));
    };
    if c.contactcraft_id.is_empty() {
        return Some(Err("Choose a ContactCraft contact.".into()));
    }
    let source = c.contactcraft_id.clone();
    let action = match command {
        "people.delete" => {
            app.session.mailbox.contacts.remove(&id);
            if app.session.sel.contact == Some(id) {
                app.session.sel.contact = None;
            }
            SharedAction::Delete(source)
        }
        "people.favorite" => {
            let value = match params.get("value") {
                Some(v) => match v.as_bool() {
                    Some(b) => b,
                    None => return Some(Err("value must be true or false".into())),
                },
                None => !c.favorite,
            };
            if let Ok(mutable) = app.session.mailbox.contact_mut(id) {
                mutable.favorite = value;
            }
            SharedAction::Favorite { id: source, value }
        }
        "people.categorize" => {
            let Some(name) = params.get("category").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty() && s.len() <= 200) else {
                return Some(Err("category must be a nonempty name under 200 bytes".into()));
            };
            let present = c.categories.iter().any(|x| x.eq_ignore_ascii_case(name));
            let value = match params.get("value") {
                Some(v) => match v.as_bool() {
                    Some(b) => b,
                    None => return Some(Err("value must be true or false".into())),
                },
                None => !present,
            };
            if let Ok(mutable) = app.session.mailbox.contact_mut(id) {
                mutable.categories.retain(|x| !x.eq_ignore_ascii_case(name));
                if value {
                    mutable.categories.push(name.to_string());
                }
            }
            SharedAction::Category { id: source, name: name.to_string(), value }
        }
        _ => return None,
    };
    app.session.transient_contacts_changed();
    Some(submit(app, action))
}

/// The persisted setting. The ContactCraft choice is offered only when the host found it.
pub(crate) fn ui_command(app: &mut MailApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != "ui.contactManager" {
        return None;
    }
    Some(match params.get("value").and_then(Value::as_str) {
        Some("included") => {
            app.session.prefs.contactcraft_contacts = false;
            forget(app);
            Ok(json!({"value": "included"}))
        }
        Some("contactcraft") if app.services.contactcraft_load.is_some() && app.services.contactcraft_action.is_some() => {
            app.session.prefs.contactcraft_contacts = true;
            app.session.view.people_account = mailcraft_model::AccountChoice::All;
            app.session.sel.contact = None;
            app.contactcraft.revision = -1;
            app.contactcraft.next_check = i64::MIN;
            app.session.transient_contacts_changed();
            Ok(json!({"value": "contactcraft"}))
        }
        Some("contactcraft") => Err("ContactCraft isn't installed on this computer.".into()),
        _ => Err("value must be included or contactcraft".into()),
    })
}

#[cfg(test)]
#[path = "contactcraft_tests.rs"]
mod tests;

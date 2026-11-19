// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! An optional live view of ContactSolutions's shared contacts. The desktop host owns the store
//! and does every disk access on a background thread; the window receives snapshots only.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, TryRecvError};

use mailsolutions_model::{Contact, Id};
use serde_json::{Value, json};

use crate::MailApp;

/// A changed ContactSolutions revision and its contacts, converted by the desktop host.
pub struct SharedContacts {
    pub revision: i64,
    pub contacts: Vec<Contact>,
}

/// Work the desktop host performs against ContactSolutions's store or editor.
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
    app.session.prefs.contactsolutions_contacts && app.services.contactsolutions_load.is_some()
}

fn forget(app: &mut MailApp) {
    let selected = app.session.sel.contact;
    app.session.mailbox.contacts.retain(|_, c| c.contactsolutions_id.is_empty());
    if selected.is_some_and(|id| app.session.mailbox.contact(id).is_none()) {
        app.session.sel.contact = None;
    }
    app.people_picked.clear();
    app.contactsolutions.ids.clear();
    app.contactsolutions.revision = -1;
    app.contactsolutions.load = None;
    app.contactsolutions.next_check = i64::MIN;
    app.session.view.people_account = mailsolutions_model::AccountChoice::All;
    app.session.transient_contacts_changed();
}

fn install(app: &mut MailApp, snapshot: SharedContacts) {
    let selected =
        app.session.sel.contact.and_then(|id| app.session.mailbox.contact(id)).map(|c| c.contactsolutions_id.clone()).filter(|id| !id.is_empty());
    app.session.mailbox.contacts.retain(|_, c| c.contactsolutions_id.is_empty());
    let mut ids = BTreeMap::new();
    for mut c in snapshot.contacts.into_iter().take(20_000) {
        if c.contactsolutions_id.is_empty() {
            continue;
        }
        let source = c.contactsolutions_id.clone();
        if let Some(old) = app.contactsolutions.ids.get(&source) {
            c.id = *old;
        }
        let id = app.session.mailbox.add_contact(c);
        ids.insert(source, id);
    }
    if let Some(source) = selected {
        app.session.sel.contact = ids.get(&source).copied();
    }
    app.contactsolutions.ids = ids;
    app.contactsolutions.revision = snapshot.revision;
    app.people_picked.clear();
    app.session.transient_contacts_changed();
}

/// Poll without blocking the frame. ContactSolutions may be closed; its WAL store remains available.
pub(crate) fn tick(app: &mut MailApp, ctx: &egui::Context) {
    if !active(app) {
        return;
    }
    if app.contactsolutions.opening != app.session.openings() {
        app.contactsolutions.opening = app.session.openings();
        app.contactsolutions.revision = -1;
        app.contactsolutions.ids.clear();
        app.contactsolutions.load = None;
        app.contactsolutions.next_check = i64::MIN;
    }
    let mut changed = false;
    let mut errors = Vec::new();
    app.contactsolutions.actions.retain_mut(|rx| match rx.try_recv() {
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
            errors.push("ContactSolutions stopped before completing the change.".to_string());
            changed = true;
            false
        }
        Err(TryRecvError::Empty) => true,
    });
    for error in errors {
        app.status(error);
    }
    if changed {
        app.contactsolutions.revision = -1;
        app.contactsolutions.load = None;
        app.contactsolutions.next_check = i64::MIN;
    }
    if let Some(rx) = &app.contactsolutions.load {
        match rx.try_recv() {
            Ok(Ok(Some(snapshot))) => {
                app.contactsolutions.load = None;
                install(app, snapshot);
            }
            Ok(Ok(None)) => {
                app.contactsolutions.load = None;
            }
            Ok(Err(e)) => {
                app.contactsolutions.load = None;
                app.status(format!("ContactSolutions contacts couldn't be read: {e}"));
                app.contactsolutions.next_check = app.session.now().saturating_add(15);
            }
            Err(TryRecvError::Disconnected) => {
                app.contactsolutions.load = None;
                app.contactsolutions.next_check = app.session.now().saturating_add(15);
            }
            Err(TryRecvError::Empty) => {}
        }
    }
    let now = app.session.now();
    if app.contactsolutions.load.is_none()
        && now >= app.contactsolutions.next_check
        && let Some(load) = &app.services.contactsolutions_load
    {
        app.contactsolutions.load = Some(load(app.contactsolutions.revision));
        app.contactsolutions.next_check = now.saturating_add(2);
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(2));
}

fn submit(app: &mut MailApp, action: SharedAction) -> Result<Value, String> {
    let Some(host) = &app.services.contactsolutions_action else {
        return Err("ContactSolutions isn't installed on this computer.".into());
    };
    app.contactsolutions.actions.push(host(action));
    Ok(json!({"pending": "contactsolutions"}))
}

/// An owner's New/Edit/Add Sender action opens ContactSolutions's full editor.
pub(crate) fn by_hand(app: &mut MailApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !active(app) {
        return None;
    }
    let action = match id {
        "people.new" if params.as_object().is_none_or(serde_json::Map::is_empty) => SharedAction::New,
        "people.edit" if params.as_object().is_none_or(serde_json::Map::is_empty) => {
            let selected = app.session.sel.contact.and_then(|id| app.session.mailbox.contact(id));
            let Some(source) = selected.map(|c| c.contactsolutions_id.clone()).filter(|s| !s.is_empty()) else {
                return Some(Err("Select a ContactSolutions contact first.".into()));
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
        "people.merge" => return Some(Err("Merge contacts in ContactSolutions while it is the contacts manager.".into())),
        _ => return None,
    };
    Some(submit(app, action))
}

/// Programmatic changes never silently write the included mailbox while ContactSolutions is chosen.
pub(crate) fn execute(app: &mut MailApp, command: &str, params: &Value) -> Option<Result<Value, String>> {
    if !active(app) {
        return None;
    }
    if matches!(command, "people.new" | "people.edit" | "people.fromMessage" | "people.merge") {
        return Some(Err("ContactSolutions is the contacts manager. Open its editor in MailSolutions to add or edit contacts.".into()));
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
    if c.contactsolutions_id.is_empty() {
        return Some(Err("Choose a ContactSolutions contact.".into()));
    }
    let source = c.contactsolutions_id.clone();
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

/// The persisted setting. The ContactSolutions choice is offered only when the host found it.
pub(crate) fn ui_command(app: &mut MailApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != "ui.contactManager" {
        return None;
    }
    Some(match params.get("value").and_then(Value::as_str) {
        Some("included") => {
            app.session.prefs.contactsolutions_contacts = false;
            forget(app);
            Ok(json!({"value": "included"}))
        }
        Some("contactsolutions") if app.services.contactsolutions_load.is_some() && app.services.contactsolutions_action.is_some() => {
            app.session.prefs.contactsolutions_contacts = true;
            app.session.view.people_account = mailsolutions_model::AccountChoice::All;
            app.session.sel.contact = None;
            app.contactsolutions.revision = -1;
            app.contactsolutions.next_check = i64::MIN;
            app.session.transient_contacts_changed();
            Ok(json!({"value": "contactsolutions"}))
        }
        Some("contactsolutions") => Err("ContactSolutions isn't installed on this computer.".into()),
        _ => Err("value must be included or contactsolutions".into()),
    })
}

#[cfg(test)]
#[path = "contactsolutions_tests.rs"]
mod tests;

// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows host for ContactCraft's optional shared contacts. All store access after startup is
//! called by `background`, so the MailCraft window never waits for SQLite or the editor.

use std::process::Command;

use contactcraft_store::{Contact, Store};
use mailcraft_ui_egui::{SharedAction, SharedContacts};

pub fn installed() -> bool {
    let standard_install = ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .flat_map(|base| [base.join("ContactCraft/ContactCraft.exe"), base.join("Programs/ContactCraft/ContactCraft.exe")])
        .any(|path| path.is_file());
    standard_install
        || (Store::default_dir().join("contacts.db").is_file() && Store::open_default().ok().and_then(|store| store.contactcraft_exe()).is_some())
}

fn converted(c: Contact) -> mailcraft_model::Contact {
    let organization = c.organizations.first();
    mailcraft_model::Contact {
        contactcraft_id: c.id.clone(),
        display_name: c.display_name(),
        first: c.name.given,
        middle: c.name.middle,
        last: c.name.family,
        nickname: c.name.nickname,
        company: organization.map(|o| o.company.clone()).unwrap_or_default(),
        title: organization.map(|o| o.title.clone()).unwrap_or_default(),
        department: organization.map(|o| o.department.clone()).unwrap_or_default(),
        emails: c.emails.into_iter().map(|e| mailcraft_model::Labeled::new(e.label, e.address)).collect(),
        phones: c.phones.into_iter().map(|p| mailcraft_model::Labeled::new(p.label, p.value)).collect(),
        addresses: c
            .addresses
            .into_iter()
            .map(|a| mailcraft_model::PostalAddress {
                label: a.label,
                street: a.street,
                city: a.city,
                region: a.region,
                postal_code: a.postal_code,
                country: a.country,
            })
            .collect(),
        websites: c.websites.into_iter().map(|w| w.value).collect(),
        birthday: mailcraft_time::Date::parse_iso(&c.birthday),
        notes: c.notes,
        categories: c.labels,
        favorite: c.favorite,
        ..Default::default()
    }
}

pub fn load(last_revision: i64) -> Result<Option<SharedContacts>, String> {
    let store = Store::open_default()?;
    let revision = store.revision();
    if revision == last_revision {
        return Ok(None);
    }
    let contacts = store.list()?.into_iter().take(20_000).map(converted).collect();
    Ok(Some(SharedContacts { revision, contacts }))
}

fn edit(store: &Store, args: &[&str]) -> Result<(), String> {
    let exe = store.contactcraft_exe().ok_or("ContactCraft isn't installed on this computer.")?;
    let answer = Command::new(exe).args(args).output().map_err(|e| format!("ContactCraft couldn't start: {e}"))?;
    match answer.status.code() {
        Some(0 | 1) => Ok(()),
        _ => {
            Err(String::from_utf8_lossy(&answer.stdout).trim().strip_prefix("error ").unwrap_or("ContactCraft couldn't open its editor.").to_string())
        }
    }
}

pub fn act(action: SharedAction) -> Result<(), String> {
    let store = Store::open_default()?;
    match action {
        SharedAction::New => edit(&store, &["--new-contact"]),
        SharedAction::Edit(id) => edit(&store, &["--edit-contact", &id]),
        SharedAction::EditEmail(email) => edit(&store, &["--edit-email", &email]),
        SharedAction::Delete(id) => store.delete(&id),
        SharedAction::Favorite { id, value } => {
            let mut c = store.get(&id)?.ok_or("The contact is no longer in ContactCraft.")?;
            c.favorite = value;
            store.save(&mut c)
        }
        SharedAction::Category { id, name, value } => {
            let mut c = store.get(&id)?.ok_or("The contact is no longer in ContactCraft.")?;
            c.labels.retain(|label| !label.eq_ignore_ascii_case(&name));
            if value {
                c.labels.push(name);
            }
            store.save(&mut c)
        }
    }
}

// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Exercise the published v1.3.0 store in an isolated directory; never launch its editor.

use super::*;

#[test]
fn published_store_contract_keeps_identity_and_shared_edits() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    let mut contact = Contact::default();
    contact.name.given = "Synthetic".into();
    contact.name.family = "Person".into();
    store.save(&mut contact).unwrap();
    let id = contact.id.clone();
    assert!(!id.is_empty());
    let view = load_from(&store, -1).unwrap().unwrap();
    assert_eq!(view.contacts.len(), 1);
    assert_eq!(view.contacts[0].contactsolutions_id, id);
    assert!(load_from(&store, view.revision).unwrap().is_none());
    act_on(&store, SharedAction::Favorite { id: id.clone(), value: true }).unwrap();
    act_on(&store, SharedAction::Category { id: id.clone(), name: "Synthetic group".into(), value: true }).unwrap();
    let reread = store.get(&id).unwrap().unwrap();
    assert!(reread.favorite);
    assert!(reread.labels.contains(&"Synthetic group".to_string()));
    assert!(load_from(&store, view.revision).unwrap().is_some());
    let editor = temp.path().join("ContactSolutions.exe");
    std::fs::write(&editor, b"synthetic discovery fixture, never executed").unwrap();
    store.set_meta("contactsolutions.exe", editor.to_str()).unwrap();
    assert_eq!(store.contactsolutions_exe().as_ref(), Some(&editor));
    act_on(&store, SharedAction::Delete(id.clone())).unwrap();
    assert!(store.list().unwrap().is_empty());
    assert!(act_on(&store, SharedAction::Favorite { id, value: true }).is_err());
}

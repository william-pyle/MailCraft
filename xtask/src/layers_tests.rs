// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn c(name: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
    Crate { name: name.into(), deps: deps.iter().map(|(n, k, w)| Dep { name: (*n).into(), kind: *k, workspace: *w }).collect() }
}
use DepKind::*;

#[test]
fn clean_downward_graph_passes() {
    let g = [
        c("mailsolutions-time", &[("kurbo", Normal, false)]),
        c("mailsolutions-model", &[("mailsolutions-time", Normal, true)]),
        c("mailsolutions-engine", &[("mailsolutions-model", Normal, true), ("mailsolutions-testkit", Dev, true)]),
        c("mailsolutions-ui-egui", &[("mailsolutions-engine", Normal, true), ("egui", Normal, false)]),
        c("mailsolutions-cli", &[("mailsolutions-ui-egui", Normal, true)]),
    ];
    assert!(check(&g).is_empty(), "{:?}", check(&g));
}

#[test]
fn upward_dependency_flagged() {
    let v = check(&[c("mailsolutions-model", &[("mailsolutions-engine", Normal, true)])]);
    assert!(matches!(v[..], [Violation::Upward { from: 1, to: 4, .. }]));
}

#[test]
fn sideways_dependency_flagged() {
    let v = check(&[c("mailsolutions-mime", &[("mailsolutions-ical", Normal, true)])]);
    assert!(matches!(v[..], [Violation::Upward { from: 2, to: 2, .. }]));
}

#[test]
fn l0_foundation_chain_allowed_one_way() {
    assert!(check(&[c("mailsolutions-model", &[("mailsolutions-time", Normal, true)])]).is_empty());
    let v = check(&[c("mailsolutions-time", &[("mailsolutions-model", Normal, true)])]);
    assert!(matches!(v[..], [Violation::Upward { from: 0, to: 1, .. }]));
}

#[test]
fn self_dev_dependency_ignored() {
    assert!(check(&[c("mailsolutions-model", &[("mailsolutions-model", Dev, true)])]).is_empty());
}

#[test]
fn upward_dev_dependency_flagged() {
    let v = check(&[c("mailsolutions-time", &[("mailsolutions-model", Dev, true)])]);
    assert!(matches!(v[..], [Violation::Upward { kind: Dev, .. }]));
}

#[test]
fn ui_crates_below_l6_flagged() {
    for dep in ["egui", "eframe", "winit", "egui_kittest", "rfd", "bevy_ecs", "bevy"] {
        let v = check(&[c("mailsolutions-engine", &[(dep, Normal, false)])]);
        assert!(matches!(v[..], [Violation::UiBelowL6 { layer: 4, .. }]), "{dep}");
    }
    assert!(check(&[c("mailsolutions-engine", &[("egui_extras_not", Normal, false)])]).is_empty());
    assert!(!check(&[c("mailsolutions-mcp", &[("winit", Normal, false)])]).is_empty());
}

#[test]
fn unregistered_crate_is_error() {
    let v = check(&[c("mailsolutions-mystery", &[])]);
    assert!(matches!(&v[..], [Violation::Unregistered { krate }] if krate == "mailsolutions-mystery"));
    assert!(v[0].to_string().contains("register"));
}

#[test]
fn testkit_only_as_dev_dependency() {
    let v = check(&[c("mailsolutions-proto", &[("mailsolutions-testkit", Normal, true)])]);
    assert!(matches!(v[..], [Violation::TestkitAsNormalDep { .. }]));
    assert!(check(&[c("mailsolutions-proto", &[("mailsolutions-testkit", Dev, true)])]).is_empty());
    // testkit itself may use anything up to L5 but not L6 crates.
    assert!(check(&[c("mailsolutions-testkit", &[("mailsolutions-engine", Normal, true)])]).is_empty());
    assert!(!check(&[c("mailsolutions-testkit", &[("mailsolutions-ui-egui", Normal, true)])]).is_empty());
}

#[test]
fn apps_and_xtask_exempt() {
    for app in ["mailsolutions", "mailsolutions-cli", "mailsolutions-web", "xtask"] {
        assert!(check(&[c(app, &[("egui", Normal, false), ("mailsolutions-ui-egui", Normal, true)])]).is_empty());
    }
}

#[test]
fn dispatch2_is_for_the_apps_alone() {
    assert!(check(&[c("mailsolutions", &[("dispatch2", Normal, false)])]).is_empty());
    for krate in ["mailsolutions-ui-egui", "mailsolutions-engine", "mailsolutions-time"] {
        let v = check(&[c(krate, &[("dispatch2", Normal, false)])]);
        assert!(matches!(&v[..], [Violation::AppOnly { dep, .. }] if dep == "dispatch2"), "{krate}: {v:?}");
    }
}

#[test]
fn the_jmap_client_sits_at_layer_3() {
    assert_eq!(classify("mailsolutions-jmap"), Some(Class::Layer(3)));
    // It may use what is below it...
    let below = [
        ("mailsolutions-time", Normal, true),
        ("mailsolutions-model", Normal, true),
        ("mailsolutions-mime", Normal, true),
        ("serde_json", Normal, false),
    ];
    assert!(check(&[c("mailsolutions-jmap", &below)]).is_empty());
    // ... and nothing at its own layer or above it, in the code or in the tests.
    for (dep, kind, to) in
        [("mailsolutions-net", Normal, 3), ("mailsolutions-proto", Normal, 3), ("mailsolutions-engine", Dev, 4), ("mailsolutions-ui-egui", Normal, 6)]
    {
        let v = check(&[c("mailsolutions-jmap", &[(dep, kind, true)])]);
        assert!(matches!(v[..], [Violation::Upward { from: 3, to: t, .. }] if t == to), "{dep}: {v:?}");
    }
    // The engine and the front end may use it.
    assert!(check(&[c("mailsolutions-engine", &[("mailsolutions-jmap", Normal, true)])]).is_empty());
    assert!(check(&[c("mailsolutions-ui-egui", &[("mailsolutions-jmap", Normal, true)])]).is_empty());
}

#[test]
fn the_ews_client_sits_at_layer_3() {
    assert_eq!(classify("mailsolutions-ews"), Some(Class::Layer(3)));
    // It may use what is below it...
    let below = [("mailsolutions-model", Normal, true), ("mailsolutions-mime", Normal, true), ("quick-xml", Normal, false)];
    assert!(check(&[c("mailsolutions-ews", &below)]).is_empty());
    // ... and nothing at its own layer or above it, in the code or in the tests.
    for (dep, kind, to) in
        [("mailsolutions-net", Normal, 3), ("mailsolutions-jmap", Normal, 3), ("mailsolutions-engine", Dev, 4), ("mailsolutions-ui-egui", Normal, 6)]
    {
        let v = check(&[c("mailsolutions-ews", &[(dep, kind, true)])]);
        assert!(matches!(v[..], [Violation::Upward { from: 3, to: t, .. }] if t == to), "{dep}: {v:?}");
    }
    // The engine may use it.
    assert!(check(&[c("mailsolutions-engine", &[("mailsolutions-ews", Normal, true)])]).is_empty());
}

#[test]
fn link_uses_names_and_not_the_other_way() {
    assert_eq!(classify("mailsolutions-link"), Some(Class::Layer(0)));
    assert!(check(&[c("mailsolutions-link", &[("mailsolutions-names", Normal, true)])]).is_empty());
    // `names` stays standalone: it may use no workspace crate.
    let v = check(&[c("mailsolutions-names", &[("mailsolutions-link", Normal, true)])]);
    assert!(matches!(&v[..], [Violation::StandaloneHasWorkspaceDep { dep, .. }] if dep == "mailsolutions-link"), "{v:?}");
    // The order is for these two alone: another crate at layer 0 may not use `link`.
    let v = check(&[c("mailsolutions-time", &[("mailsolutions-link", Normal, true)])]);
    assert!(matches!(v[..], [Violation::Upward { from: 0, to: 0, .. }]), "{v:?}");
}

#[test]
fn metadata_parsing() {
    let meta: Value = serde_json::from_str(
        r#"{"packages":[
            {"name":"mailsolutions-model","dependencies":[
                {"name":"mailsolutions-time","kind":null,"path":"/x/crates/time"},
                {"name":"serde","kind":null},
                {"name":"proptest","kind":"dev"}]},
            {"name":"mailsolutions-time","dependencies":[]}
        ]}"#,
    )
    .unwrap();
    let g = from_metadata(&meta).unwrap();
    assert_eq!(g.len(), 2);
    let doc = g.iter().find(|c| c.name == "mailsolutions-model").unwrap();
    assert!(doc.deps[0].workspace && !doc.deps[1].workspace);
    assert_eq!(doc.deps[2].kind, Dev);
    assert!(check(&g).is_empty());
}

#[test]
fn the_mailbox_adapter_uses_the_reader_and_not_the_other_way() {
    assert_eq!(classify("mailsolutions-mime-model"), Some(Class::Layer(2)));
    assert!(check(&[c("mailsolutions-mime-model", &[("mailsolutions-mime", Normal, true), ("mailsolutions-model", Normal, true)])]).is_empty());
    let v = check(&[c("mailsolutions-mime", &[("mailsolutions-mime-model", Normal, true)])]);
    assert!(matches!(v[..], [Violation::Upward { from: 2, to: 2, .. }]), "{v:?}");
    // `ber` stands on its own.
    let v = check(&[c("mailsolutions-ber", &[("mailsolutions-time", Normal, true)])]);
    assert!(matches!(&v[..], [Violation::StandaloneHasWorkspaceDep { .. }]), "{v:?}");
}

// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hostile DAV XML is bounded and never panics.
#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(root) = mailsolutions_dav::xml::parse(data) {
        let _ = mailsolutions_dav::xml::properties(&root).count();
    }
});

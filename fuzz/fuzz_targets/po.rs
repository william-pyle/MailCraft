// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! A gettext catalogue (`.po`): read it, write what was read and read that again, and look a text
//! and a plural up in it.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let file = mailsolutions_i18n::po::parse(&text);
    let again = mailsolutions_i18n::po::parse(&mailsolutions_i18n::po::write(&file.entries));
    let _ = again.entries.len();
    let catalogue = mailsolutions_i18n::Catalogue::parse(&text);
    let _ = catalogue.get("Reply");
    let _ = catalogue.get_plural("{n} message", 2);
    let _ = catalogue.counts();
});

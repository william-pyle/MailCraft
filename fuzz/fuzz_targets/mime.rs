// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! A message (and whether it is Digitally signed or Encrypted), an mbox file, a `mailto:` link and
//! encoded words, from any bytes: by the reader on its own types, and as the mailbox's messages.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = mailsolutions_mime::parse(data);
    let _ = mailsolutions_mime::parse_fetched(data);
    let _ = mailsolutions_mime::protection(data);
    let _ = mailsolutions_mime::mbox_messages(data).count();
    let _ = mailsolutions_mime_model::parse_fetched(data);
    let _ = mailsolutions_mime_model::parse_mbox(data);
    let text = String::from_utf8_lossy(data);
    let _ = mailsolutions_mime::parse_mailto(&text);
    let _ = mailsolutions_mime::decode_words(&text);
});

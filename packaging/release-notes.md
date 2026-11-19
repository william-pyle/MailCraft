# MailSolutions 0.14.0 in short

- MailCraft is now **MailSolutions** across the app, executables, installer and project.
- The familiar Rust/egui mail, calendar, contacts and tasks workflow remains in place.
- Shared contacts now use the published **ContactSolutions 1.3.0** store and editor.
- Guarded profile migration retains original data, settings, mailbox IDs and recovery evidence.
- Existing sign-in credential bindings and checksum-signing trust are retained.
- **The first upgrade needs a manual download**; MailCraft 0.13.2 cannot follow the renamed API or asset names.

**What's new**

- The Windows profile is `%APPDATA%/MailSolutions`. Migration validates an offline copy before activating it, refuses busy/conflicting/damaged profiles, preserves the old folder, and quarantines old pending updates.
- Source and packages use `mailsolutions`; the existing GitHub repository is `william-pyle/MailSolutions`. Existing releases and tags remain available.

**Fixes**

- The ContactSolutions option keeps the previous saved choice and uses the new executable-discovery and shared-store contracts.
- Settings saves retain unknown fields. Reporting a problem opens this fork's GitHub issue page; no upstream report relay is configured.

Read [migration and recovery](https://github.com/william-pyle/MailSolutions/blob/v0.14.0/docs/rename-migration.md) before the initial upgrade. Keep a complete backup of this app's profile and any separately located mailboxes. A ContactSolutions backup is not a MailSolutions backup.

Windows x64 only. Checksums are signed with the existing release key; Windows publisher signing remains unconfigured. Packages are inspected without installation. Interactive/unattended upgrade, native installed interaction, real sign-in continuity and live-data migration are not independently verified by this release.

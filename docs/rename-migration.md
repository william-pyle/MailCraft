# MailSolutions 0.14.0 migration and recovery

MailCraft is now MailSolutions. The Rust layers, egui interaction flow, command IDs,
mailbox schema, accounts and record IDs remain the same. Contact integration uses
the published `contactsolutions-store` 1.3.0, its `CONTACTSOLUTIONS_DATA` setting,
`contacts.db`, `contactsolutions.exe` metadata and `--new-contact`, `--edit-contact`
and `--edit-email` editor commands. MessageSolutions is a separately maintained peer.

## First start and recovery

The Windows profile is now `%APPDATA%/MailSolutions`. Before reading preferences,
update state or the mailbox, an ordinary start checks `%APPDATA%/MailCraft`. When
only the old profile exists, the application copies it to a staging directory,
holding every source file against writing/deletion on Windows. A busy file causes
a visible refusal; the application never closes another process.

Every file is SHA-256 checked after copying. Only the staged settings/mailbox are
parsed for validation. Account, message, folder and contact IDs are preserved.
The mailbox remains `mailbox.mailcraft`, including its blob and search sidecars;
this is a stable file format, not current application branding. Recent-file paths
inside the copied profile are remapped; separately located mailboxes stay where
their owner placed them. The retained original is not the active store. Unknown
settings are carried into the copied JSON, and the old `contactcraftContacts`
preference is read as the renamed contacts option.

Only the validated stage is renamed into place. `rename-receipt.json` records the
original path and file hashes. Later starts check that the retained source still
matches that receipt, refusing divergent profiles. Old downloaded/pending updates
are quarantined as `updates.pre-rename` in the copy, never applied as a new update.

Both profiles without a matching receipt, invalid settings/mailbox content,
inaccessible files, reparse points, and leftover `.migration-staging` or
`.migration-lock` paths stop the migration. Preserve both profiles and all staged
files. Confirm that no migration is running before moving an interrupted stage or
lock aside for review; never blindly delete them or merge profiles. Retry from a
verified original. Migration needs room for a complete copy; originals are never
deleted. No backup of ContactSolutions substitutes for a backup of this profile.

Before rollback, preserve the new profile and all later edits. The retained old
profile contains only the state at cutover. Restoring it alone loses later work;
reconcile that work before reverting. Installation rollback does not roll data back.
Neither `--sample`, `--version`, nor runs with `MAILSOLUTIONS_NO_PREFS` migrate a profile.
Automated tests use explicit synthetic directories and mock credentials only.

## Stable contracts and deliberate exceptions

- Credential service `MailCraft`, existing account-specific keys and OAuth
  registration identity are retained, preventing loss of remembered sign-ins.
  No credential contents are copied, logged or migrated by this rename.
- `.mailcraft`, `MailCraft index` format signatures, and private `X-MailCraft-*`
  mail/vCard metadata remain stable so existing files and in-flight server records
  remain readable. New application crates/executables are `mailsolutions*`.
- MSI UpgradeCode `647FB3CC-7876-4DC8-84D9-CE2DD295D1C7` is retained. Application
  and file-association components have new GUIDs because their key paths changed.
  The unchanged `Software/Policies/MailCraft` update policy and its component GUID
  remain effective. The installer installs to Program Files/MailSolutions.
- The paused Android application ID and snapshot namespace `work.mailcraft.android`
  remain stable to preserve its package identity; no mobile release is made.
- Existing GitHub signing/OAuth secret bindings map into renamed build variables.
  The public checksum-signing keys are unchanged. There is no new credential setup.
- Legal authorship, upstream repository identity, historical records, releases
  and tags retain their names. Alpha keeps stable ID `mailcraft`, its canonical
  record folder and `#/mailcraft/0` and `#/mailcraft/1` routes.
- The owner authorized enabling Issues on this fork. Report a Problem opens its
  GitHub issue form; the upstream report relay is disabled in this build. No
  report is sent by this migration.

## Release and upgrade limits

The existing GitHub repository is renamed to `william-pyle/MailSolutions` after
local validation. Version 0.14.0 is a normal release from an exact commit with a
passing `ci-ok`, through the signed-checksum release workflow. Source moves to
`C:/Projects/MailSolutions` with its `.git` history intact.

MailCraft 0.13.2 does not follow GitHub API redirects and expects old asset names.
The initial upgrade therefore requires a manual download of the new MSI; it is
not advertised as a working in-app update from the old version. MailSolutions
resolves its own future releases and names. The same MSI upgrade family lets
Windows Installer identify the prior product; actual interactive/unattended
upgrade, uninstall, shortcuts and pinned entries still need isolated acceptance.

The release checks package contents, PE resources and MSI tables without installing
anything. It does not prove a native installed app works, a real sign-in survives,
or the owner's live data has migrated. No install, uninstall, real-account login,
send, live-profile launch, session closure or restart is part of this rename.
Windows publisher signing is still unconfigured; checksum signatures are distinct
from an Authenticode publisher signature. Non-Windows builds remain paused.

The pinned ContactSolutions 1.3.0 store brings chrono 0.4.45. RustSec
[RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543) is an
informational unmaintained notice with no patched version. A dated, scoped
exception is recorded in both dependency scanners; vulnerability checks remain
enabled. Re-review this dependency with the next ContactSolutions store release.
Replacing chrono in that shared store is separate work, not part of this rename.

## Reproduce the public download check

After publication, run `cargo run --locked -p mailsolutions-net --example
release_verify -- <new-output-directory>`. It uses the actual anonymous updater
API parser, validates the release version, verifies signed checksums with the
compiled public keys, downloads the MSI and ZIP, and reports their SHA-256 values.
It never installs or opens an application profile. Inspect the MSI with
`packaging/windows/verify-package.ps1 -Msi <downloaded-msi>`.

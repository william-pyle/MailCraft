#!/usr/bin/env bash
# Offline signature/byte tests with an ephemeral synthetic key, never the release key.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -f -- "$tmp/key.der" "$tmp/key2.der" "$tmp/keys.rs" "$tmp/file" "$tmp/SHA256SUMS.txt" "$tmp/SHA256SUMS.txt.sig"; rmdir -- "$tmp"' EXIT
openssl genpkey -algorithm ED25519 -outform DER -out "$tmp/key.der" 2>/dev/null
public="$(openssl pkey -in "$tmp/key.der" -inform DER -pubout -outform DER 2>/dev/null | tail -c 32 | base64 | tr -d '\n')"
printf 'pub const RELEASE_KEYS: [&str; 1] = ["%s"];\n' "$public" >"$tmp/keys.rs"
printf 'synthetic release bytes' >"$tmp/file"
(cd "$tmp" && sha256sum file >SHA256SUMS.txt)
sign() { openssl pkeyutl -sign -rawin -keyform DER -inkey "$1" -in "$tmp/SHA256SUMS.txt" | base64 >"$tmp/SHA256SUMS.txt.sig"; }
reject() { if bash "$root/packaging/verify-sums.sh" "$tmp" "$tmp/keys.rs" >/dev/null 2>&1; then echo "accepted $1" >&2; exit 1; fi; }
sign "$tmp/key.der"
bash "$root/packaging/verify-sums.sh" "$tmp" "$tmp/keys.rs"
printf 'changed bytes' >"$tmp/file"
reject 'changed final package'
printf 'synthetic release bytes' >"$tmp/file"
printf '\nchanged manifest' >>"$tmp/SHA256SUMS.txt"
reject 'changed checksum manifest'
(cd "$tmp" && sha256sum file >SHA256SUMS.txt)
openssl genpkey -algorithm ED25519 -outform DER -out "$tmp/key2.der" 2>/dev/null
sign "$tmp/key2.der"
reject 'untrusted signer'
: >"$tmp/SHA256SUMS.txt.sig"
reject 'empty signature'
echo 'signature verification: valid bytes passed; four negative cases rejected'

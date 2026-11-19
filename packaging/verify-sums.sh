#!/usr/bin/env bash
# Verify the exact final signed bytes with the public keys the updater trusts.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(cd "${1:?usage: verify-sums.sh <release-files> [test-public-key-source]}" && pwd)"
keys="${2:-$root/crates/net/src/update.rs}"
tmp="$(mktemp -d)"
trap 'rm -f -- "$tmp/signature" "$tmp/public.der"; rmdir -- "$tmp"' EXIT
[[ -s "$dir/SHA256SUMS.txt.sig" ]] || { echo 'missing checksum signature' >&2; exit 1; }
base64 -d <"$dir/SHA256SUMS.txt.sig" >"$tmp/signature"
[[ $(wc -c <"$tmp/signature") -eq 64 ]] || { echo 'invalid signature length' >&2; exit 1; }
verified=false
while IFS= read -r key; do
  { printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'; printf '%s' "$key" | base64 -d; } >"$tmp/public.der"
  if openssl pkeyutl -verify -rawin -pubin -keyform DER -inkey "$tmp/public.der" -sigfile "$tmp/signature" -in "$dir/SHA256SUMS.txt" >/dev/null 2>&1; then
    verified=true
  fi
done < <(sed -n '/^pub const RELEASE_KEYS:/p' "$keys" | grep -oE '"[A-Za-z0-9+/]{43}="' | tr -d '"')
[[ $verified == true ]] || { echo 'checksum signature is not trusted by this updater' >&2; exit 1; }
(cd "$dir" && sha256sum --check --strict SHA256SUMS.txt)
echo 'Final release bytes and updater-trusted signature verified'

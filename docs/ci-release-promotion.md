# CI timing, inputs and release promotion

The owner authorized CI-PROMOTION on 2026-11-19. This changes future workflows;
it does not authorize a new tag, publication, installation or restart. Published
0.14.0 remains unchanged.

## Measured baseline

| Phase | 0.14.0 | Previous 0.13.2 |
|---|---:|---:|
| Entire CI run | 44m43s | 44m36s |
| Windows setup, checkout, tools and cache restore | about 1m16s | about 56s |
| Clippy | 6m17s | 6m45s |
| Compile/link test binaries | 24m55s | 26m45s |
| Run nextest tests | 8m04.664s | 8m12.570s |
| Compile documentation-test dependencies | 33.67s | 32.02s |
| Documentation tests and small structural gates | about 10s | about 10s |
| Cache save/cleanup step | 2m13s | 1s |
| Entire release run | 14m32s | 14m34s |
| Optimized release build step | 11m01s | 11m29s |
| Sign/package MSI and ZIP step | 57s | 50s |
| Final package check job | 21s | 11s (older check procedure) |
| Checksum/provenance/publish job | 35s | 24s |

Sources: [0.14.0 CI](https://github.com/william-pyle/MailSolutions/actions/runs/44444187007),
[0.13.2 CI](https://github.com/william-pyle/MailSolutions/actions/runs/43795092845),
[0.14.0 release](https://github.com/william-pyle/MailSolutions/actions/runs/44449722667),
[0.13.2 release](https://github.com/william-pyle/MailSolutions/actions/runs/43799652143).
The latest run spent about 28 seconds outside its Windows job, including its change
gate, scheduling gaps and final ci-ok barrier; this is not 28 seconds of pure queue
time. Its first job began four seconds after creation. The deny job ran concurrently
for 3m08s, including 2m03s compiling xtask before the advisory check.

Both CI runs restored the same approximately 637 MB dependency cache. The latest
was a fallback-key hit after the rename; the previous was an exact hit. An exact
hit therefore did not remove the long test compilation. Workspace crates are not
cached by the existing rust-cache defaults. No measured warm-workspace-cache speedup
is claimed. More cache data also costs restore/save time and storage; existing
dependency caching is retained, and production builds remain cache-free.

Cargo's parallel compilation log does not report exclusive dependency, application
code-generation and linker durations. In the latest run the application compilation
started about 21m47s before test-build completion; in release it started about 7m11s
before completion. Those are overlapping windows, **not** measured linker costs.
Compiler timing/profiling in a future necessary run would resolve the split. The
slowest individual test was synthetic 200,000-message interruption/recovery at
86.223s; the entire suite ran in 484.664s. Nothing here justifies dropping tests.

The same commit also had an [unsigned dry run](https://github.com/william-pyle/MailSolutions/actions/runs/44445327149)
lasting 14m42s, including an independent 11m55s build step. That optimized build
was repeated by the tagged release. The new flow removes that routine duplication.
Removing the observed release build job leaves roughly three minutes of existing
release work **before** new promotion verification and queue variance. This is an
estimate, not an observed new release duration; no release is run to benchmark it.

## Declared build inputs

The inspected active jobs check out this repository at the workflow SHA. No checkout
has a different `repository:` input. There are no Git submodules. All 76 path entries
in the 35 tracked Cargo manifests resolve within this checkout (including auxiliary
manifests). The root lockfile describes 32 workspace packages, 707 crates.io packages
and three packages from two declared Git sources; it includes other supported targets,
so that is not a claim that Windows compiles all 742 packages.

The cross-project source input is **contactsolutions-store**, from the published
ContactSolutions v1.3.0 repository, locked to
`17245278eef33d2be5fd57f227dadcd6b941bcf3`. It is not a sibling local checkout.
The windowing fork at `UCS-Care-Group/winit` is pinned to
`1477b126a8c3d3d2add446c93492454ba833074d` (two lockfile packages). Registry packages
have lockfile checksums; compilation and quality commands use `--locked`.

Other downloads are declared tooling: Rust 1.98.1/rustfmt/Clippy,
cargo-nextest 0.9.150, cargo-deny 0.20.2, pinned GitHub Actions, GitHub cache/artifact
storage, advisory databases, and WiX 5.0.2 plus its hash-pinned NuGet extensions.
The optional Windows signing client is also pinned by version/hash; Azure signing
was not configured in the observed release. Paused non-Windows jobs do not execute
their listed downloads.

The three project build scripts read local icon/version resources, local translation
catalogs, and local contributor records; the Windows resource compiler comes from
the runner SDK. No script was found that searches unrelated project folders or
gathers information for compilation. This is a configuration/log audit, **not** a
network trace, a sandbox guarantee or an audit of every transitive dependency's
build script. No unrelated project contents were inspected.

## Promotion contract

1. Complete source, version, release notes and workflow changes before committing.
   Run the required local gate once on the final change, push to the owner origin,
   and observe exact-commit main CI. Do not dispatch CI again if that commit already
   has a running or successful compatible run.
2. Trusted main push/manual CI builds one optimized Windows candidate in parallel
   with the existing quality and advisory gates. It gets the existing desktop OAuth
   build configuration, no signing credentials and no release environment. Both
   executables answer `--version`; the GUI's commit and version resources are checked.
3. CI uploads a uniquely named immutable candidate with a receipt and SHA-256 for
   each binary. It retains the candidate for 30 days. Only the same complete run
   attempt with successful quality, deny, candidate and ci-ok jobs is promotable.
4. A tag must name that exact main commit and equal the Cargo version. The verifier
   pins repository ID, workflow path/ID, branch/event, commit and run attempt; checks
   completed success, expiry, artifact identity, API archive digest and size; rejects
   extra/duplicate/unsafe ZIP entries; then checks receipt version, target, compiler,
   profile, static CRT, resource requirement, canonical Git input blobs and binary
   hashes. It never executes an artifact. A same-tree PR result is insufficient.
5. The verified programs are handed by digest to the existing isolated signing/
   packaging job. A public `mailsolutions-<version>-build.json` records their CI
   origin, archive digest and receipt, without credential values. After signing and
   packaging, a no-secret job verifies the final handoff and both version commands
   and inspects MSI identity. No installer runs.
6. Publication verifies the final package handoff, signs checksums with the existing
   release key, verifies that signature against the updater's public keys and checks
   every listed final byte. Provenance still covers final release files, including
   the CI receipt. Published releases cannot be replaced. Normal public download/
   updater verification remains required after an authorized future publication.

Dispatching `release.yml` on main is now a **verification-only preflight** of an
existing successful candidate. It performs no build, suite, packaging or publication.
The optional `ci_run_id` chooses a specific compatible successful run; otherwise the
newest successful trusted main run for this SHA is used. Missing/expired evidence,
partial reruns lacking same-attempt gates, configuration mismatch or API failure
stops promotion. There is no hidden rebuild or fallback to older/incompatible code.
If evidence has expired, explicitly authorize/trigger a fresh complete CI run before
tagging. If desktop build configuration changes, produce a fresh CI candidate; the
receipt records only whether the complete registration pair was present, not values.

Debug test binaries, Clippy analysis and optimized static-CRT production binaries
are different configurations and cannot safely substitute for each other. Local
pre-commit validation is still required by AGENTS.md; hosted exact-commit CI remains
the release authority. Main CI no longer cancels an in-flight main run; PR-only
cancellation remains enabled. Release concurrency still never cancels a running
release. The application architecture, dependencies and installer family are unchanged.

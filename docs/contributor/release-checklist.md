# Release checklist (P4)

Use before tagging **v1.0.0** (GA). Evidence-first — no tag without Alpine logs.

## Versioning (SemVer)

Single source of truth: **`[workspace.package].version`** in root [`Cargo.toml`](../../Cargo.toml). All Rust crates inherit via `version.workspace = true`.

| Surface | How to read / bump |
|---------|-------------------|
| Rust workspace | `just version` or `cargo pkgid -p nusa-cli` |
| CLI on server | `nusa --version` |
| PHP driver | `php-driver/composer.json` — keep aligned with Rust (`just sync-composer-version`) |
| Git release | Annotated tag `vX.Y.Z` (triggers [release workflow](../../.github/workflows/release.yml)) |
| Changelog | [Keep a Changelog](https://keepachangelog.com/) — move `[Unreleased]` into `## [X.Y.Z] - date` |

### Incremental bump workflow

1. Install once (host): `cargo install cargo-edit`
2. Bump: `just release-bump patch` (or `minor` / `major`)
3. Edit [`CHANGELOG.md`](../../CHANGELOG.md) — new section for the version
4. Run gates: `just podman-ci` (and `just podman-ci-e2e` for GA candidates)
5. Commit: `chore: release vX.Y.Z`
6. Tag and push: `git tag -a vX.Y.Z -m "vX.Y.Z"` then `git push origin vX.Y.Z`
7. Post-tag: row in [`docs/public/compatibility-matrix.md`](../public/compatibility-matrix.md)

Without `cargo-edit`, bump `version` in `Cargo.toml` manually, then `just sync-composer-version`.

**Rules:** PATCH = fixes/docs; MINOR = backward-compatible features; MAJOR = breaking changes (or `1.0.0` at GA). IPC `Hello.version` is a protocol contract — bump only when the handshake or message format breaks compatibility.

## Pre-tag gates

| Step | Command | Pass criteria |
|------|---------|---------------|
| 1 | `just podman-build` | Only if Dockerfile, `Cargo.lock`, or fixture deps changed |
| 2 | `just podman-ci` | fmt + lint + 1620 workspace + Laravel E2E 14/14 |
| 3 | `just podman-ci-e2e` | + leak 10k + IPC bench + `podman-bench-normal-smoke` |
| 4 | `just security` | audit + geiger + deny (host; review Alpine separately) |

Archive CI log URLs or local transcripts in the release issue.

## P3 benchmarks (same release window)

| Artifact | Action |
|----------|--------|
| [`docs/benchmarks/normal-mode-report.md`](../benchmarks/normal-mode-report.md) | wrk/k6 on same VM as optional FPM baseline |
| [`docs/benchmarks/octane-ipc-report.md`](../benchmarks/octane-ipc-report.md) | Criterion + `just podman-bench-ipc-smoke` |
| [`tests/load/`](../../tests/load/) | `k6 run tests/load/normal-static.js`, `record-wrk.sh` |

## Tag and GitHub release

1. `just release-bump patch|minor|major` (or manual bump + `just sync-composer-version`) and update `CHANGELOG.md`.
2. Push annotated tag: `git tag -a v1.0.0 -m "GA v1.0.0"` (example).
3. Push tag — [`.github/workflows/release.yml`](../../.github/workflows/release.yml) builds musl binaries + SBOM.

Verify release assets:

- `nusa-x86_64-unknown-linux-musl`
- `nusa-aarch64-unknown-linux-musl` (if matrix enabled)
- `sbom.json` (SPDX JSON from Syft)

## Post-tag

- [ ] Update [`docs/public/production-status.md`](../public/production-status.md) GA criteria table
- [ ] Publish Packagist `nusa/octane` when ready (path → semver tag aligned with Rust release)
- [ ] Announce compatibility matrix version row

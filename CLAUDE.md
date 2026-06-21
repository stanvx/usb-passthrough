# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

USB/IP passthrough bridge — export a physical USB device on one machine, import it on another as if locally attached. RFC-compliant USB/IP kernel protocol. Targets Linux, Windows, Android (phone + TV), plus a web console. Alpha status.

Strict terminology lives in `CONTEXT.md` — read it before renaming things. Architecture, thread model, and platform details: `ARCHITECTURE.md`. Wire protocol: `PROTOCOL.md`. Past + future work: `ROADMAP.md`.

## Build & Test

```bash
cargo build --release                              # full workspace (Linux: excludes `windows`)
cargo test --release --workspace                   # full test run
cargo check --workspace --exclude anyplug          # fast compile check (Linux CI does this)
cargo fmt --all -- --check                         # rustfmt is configured — see rustfmt.toml
cargo clippy --workspace -- -D warnings            # CI gate; treat warnings as errors
```

Per-crate:
```bash
cargo test --release -p usbip-core                 # only crate with real coverage today
cargo test --release -p usbip-server               # passes vacuously
cargo test --release -p usbip-client               # passes vacuously
```

Android (committed wrapper exists — `./gradlew`, not bare `gradle`):
```bash
cd android
./gradlew lintDebug
./gradlew :app:assembleDebug
./gradlew :app:assembleRelease
./gradlew :tv:assembleRelease
```

## Conventions

- Rust formatter: see `rustfmt.toml` — `max_width = 100`, `use_small_heuristics = "Max"`, edition 2021. Don't reformat against these.
- Conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`) — observed in practice.
- No emojis in code, comments, or docs.
- Prefer immutability, small files (200–400 lines typical, 800 max).
- Pre-commit gate: run `/verify` (see `.claude/skills/verify/`). For TDD: `/tdd`.
- Releases: `/release` skill handles version bumps, `CHANGELOG.md`, and tag push.

## Gotchas Claude gets wrong without this

- **`windows/Cargo.toml` does not use `version.workspace = true`** — its version is hardcoded `"0.3.0"`. The release skill bumps it manually. Don't "fix" this; the hardcoding is deliberate.
- **CI excludes the `windows` crate on Linux** (`cargo check --workspace --exclude anyplug`, `cargo test --workspace --exclude anylinux`). Builds touching `windows/` won't be exercised by CI on ubuntu-latest.
- **Release workflow uses `continue-on-error: true`** on Windows + Android-TV steps. A failed build can still produce an empty artifact — verify the artifacts tab, not the green checkmark.
- **Android JNI crate (`android/rust/usbip-android/`) is NOT a workspace member.** It's compiled by the `rust-android` Gradle plugin from `android/core/`. Don't add it to root `Cargo.toml` members.
- **`docs/adr/` numbering is inconsistent** (0002, 0004, plus a 3-digit `004-…`). Don't renumber — ADR ordering is not strict.
- **rustfmt hook is configured** in `.claude/settings.json` (PostToolUse on `.rs` edits). Just write Rust; formatting happens automatically.
- **`gradlew` wrapper is committed.** Don't recreate it or call out its absence.

## Skills (already wired in `.claude/`)

- `/verify` — pre-commit gate (check + test + clippy + fmt).
- `/tdd` — red-green-refactor workflow with crate-specific patterns.
- `/release` — version bump + changelog + tag + monitor. `disable-model-invocation: true`.

## Docs

`docs/BUILDING.md`, `docs/SETUP.md`, `docs/PERFORMANCE.md` (latency budget), `docs/TROUBLESHOOTING.md`, `docs/ANDROID-TV.md`, `docs/DOCKER.md`. Issue tracker conventions: `docs/agents/issue-tracker.md`, `docs/agents/triage-labels.md`.
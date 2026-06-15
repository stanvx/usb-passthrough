# Contributing to AnyPlug

Thanks for your interest. AnyPlug is a cross-platform USB/IP bridge written in Rust,
with a Kotlin/Android layer and a Windows GUI. We ship to gamers and home users who
depend on it for their peripherals.

For design context, read [`ARCHITECTURE.md`](ARCHITECTURE.md) and
[`PROTOCOL.md`](PROTOCOL.md); for milestones, see [`ROADMAP.md`](ROADMAP.md).

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Project Licensing](#project-licensing)
- [How to Ask Questions](#how-to-ask-questions)
- [Filing Bug Reports](#filing-bug-reports)
- [Suggesting Features](#suggesting-features)
- [Reporting Security Issues](#reporting-security-issues)
- [Development Setup](#development-setup)
- [Build and Test](#build-and-test)
- [Coding Style](#coding-style)
- [Test-Driven Development](#test-driven-development)
- [Commit Messages and PRs](#commit-messages-and-prs)
- [Areas We Especially Want Help With](#areas-we-especially-want-help-with)
- [Good First Issues](#good-first-issues)
- [Release Process](#release-process)

---

## Code of Conduct

Be respectful and constructive. Disagreement is welcome; rudeness is not.
If you are new to open source, don't worry — maintainers are patient.

## Project Licensing

Released under the **MIT License** ([`LICENSE`](LICENSE)). By contributing, you
agree your work will be distributed under the same terms.

## How to Ask Questions

Open a GitHub issue with the `question` label. Search existing issues first.

## Filing Bug Reports

Include:
- **OS and version**
- **USB device**: VID:PID and description
- **Server and client versions** (`--version` or commit SHA)
- **Exact reproduction steps**
- **Log excerpt** with `RUST_LOG=debug` (focused excerpt, not full buffer)
- **Expected vs. actual behaviour**

For enumeration issues, also include `lsusb -v` (Linux), `usbip list -l` (server),
and client errors from `dmesg` / Event Viewer / `adb logcat`.

## Suggesting Features

Open a GitHub issue with the `enhancement` label. Describe the problem first,
then the proposed solution. Reference a roadmap milestone if applicable.

## Reporting Security Issues

**Do not** open a public issue. Use [`SECURITY.md`](SECURITY.md) for the
private reporting channel (GitHub Security Advisories). Crypto issues
(AES-256-GCM, X25519, HKDF) are highest priority and triaged quickly.

---

## Development Setup

A Cargo workspace with four Rust crates plus Android and Windows subprojects.
Full prerequisites in [`docs/BUILDING.md`](docs/BUILDING.md); summary below.

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Linux: `sudo apt install build-essential pkg-config libusb-1.0-0-dev libudev-dev clang libclang-dev llvm-dev cmake`
Windows: Visual Studio 2022 Build Tools with C++ workload and Windows SDK.

### Android (optional)

```bash
export ANDROID_HOME=$HOME/Android
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653
sdkmanager "platforms;android-34" "build-tools;34.0.0" "ndk;25.2.9519653"
yes | sdkmanager --licenses
rustup target add aarch64-linux-android
```

### Clone

```bash
git clone https://github.com/stanvx/anyplug.git
cd anyplug
```

## Build and Test

Minimal checks before opening a pull request:

```bash
cargo build --release
cargo test --release
cargo clippy --all-targets -- -D warnings
```

Android (`android/`): `./gradlew assembleDebug`
Windows (`windows/`): `cargo build --release -p windows`

If you change the wire format, add a round-trip test in `shared/usbip-core/src/protocol.rs`.
The CI pipeline ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs these checks
on Linux, Windows, and Android — verify locally first.

## Adding a USB Descriptor Fixture

Test fixtures live in `shared/usbip-core/tests/fixtures/`. Each fixture is a
directory with `descriptor.bin` (raw USB descriptor bytes) and `metadata.toml`
(expected fields).

### Capturing a descriptor dump

- **Linux**: `sudo lsusb -d <VID:PID> -v` or `sudo usbhid-dump -e descriptor -d <VID:PID>`
- **Windows**: [USB Device Tree Viewer](https://www.uwe-sieber.de/usbtreeview_e.html) or `RUST_LOG=debug` server log
- **macOS**: `system_profiler SPUSBDataType` or `ioreg -p IOUSB -l -w 0`

### Creating a new fixture

1. `mkdir -p shared/usbip-core/tests/fixtures/<name>`
2. Place raw descriptor bytes in `descriptor.bin` (little-endian, USB order):
   Device (18B) > Config (9B) > Interface (9B) > [class-specific] > Endpoint (7B)
3. Create `metadata.toml` with expected fields (all optional):

   ```toml
   [device]
   vendor_id = 0x046d
   product_id = 0xc261
   device_class = 0
   max_packet_size0 = 64

   [interface]
   class = 3
   num_endpoints = 1

   [[endpoints]]
   address = 0x81
   transfer_type = "interrupt"
   ```

4. Verify: `cargo test -p usbip-core --test descriptor_fixtures`
5. Commit: `git add shared/usbip-core/tests/fixtures/<name>/ && git commit -m "test(fixtures): add <device> descriptor fixture"`

Full schema at `shared/usbip-core/tests/fixtures/README.md`.

## Coding Style

**Boring, small, immutable.**

- **No emojis** in source, comments, commits, or docs.
- **Immutability by default** — return new data, don't mutate.
- **Many small files** — 200-400 lines typical, 800 max.
- **Explicit error handling** — no `unwrap()` in non-test code. Use `Result`
  and `usbip_core::error::Error`; map foreign errors at the boundary.
- **Validate at boundaries** — treat network, USB, and filesystem input as hostile.
- **Comments explain *why*,** not *what*.

Run `cargo fmt` and `cargo clippy` before pushing. For Kotlin, follow the
[Android Kotlin style guide](https://developer.android.com/kotlin/style-guide).

## Test-Driven Development

**80% minimum coverage.** Use inline `#[cfg(test)] mod tests` blocks in source
files, not a separate `tests/` directory.

Workflow:
1. **RED**: Write the test first. It must fail meaningfully (not on `unimplemented!()`).
2. Run `cargo test -p <crate>` and watch it fail.
3. **GREEN**: Write the minimum code to pass.
4. Run `cargo test --release` for the full workspace.
5. **IMPROVE**: Refactor while keeping tests green.
6. Verify coverage: `cargo llvm-cov` (cross-platform). Target 80%.

Bug fixes must include a regression test that reproduces the bug.

## Commit Messages and PRs

### Branch and commit

- Branch from `main`. One topic per branch.
- **Conventional commits**: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`,
  `chore:`, `perf:`, `ci:`. Subject line: imperative, 50-72 characters.
- Body explains *why*. Reference issues: `Closes #123` or `Refs #123`.
- **Sign your commits.** Unsigned commits will not be merged.

### Pull request workflow

1. Open a **draft PR early** for direction feedback.
2. Before requesting review: `cargo build --release`, `cargo test --release`,
   `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` — all clean.
3. Describe what the PR does, why, and how you tested it. Link the issue.
4. **One approval** suffices for most changes. Architectural changes (wire
   format, crypto, Windows/Android service lifecycle) need a maintainer review.
5. **Squash-merge** is default. Request rebase-merge in the PR description if needed.

### What we will push back on

- PRs mixing formatting, refactoring, and behaviour changes. Split them.
- New dependencies without prior discussion.
- Crypto changes without accompanying test vectors.

---

## Areas We Especially Want Help With

1. **macOS server/client** (milestone 12) — I/O Kit enumeration, IOUSBFamily
   integration, uinput-equivalent input path.
2. **Device compatibility** — HID, mass-storage, USB-to-serial class devices.
3. **Performance on ARM** (Raspberry Pi) — URB buffer pool tuning, zero-copy,
   batched submission (milestone 11).
4. **Documentation translation** — open `docs/i18n/<lang>/` with a `TRANSLATING.md`.
5. **Test fixtures** — captured USB descriptors from real devices.

Other needs: hot-plug, isochronous transfers, IPv6, Prometheus metrics
(milestones 11, 13).

## Good First Issues

1. **Add a device profile.** Create `docs/DEVICES.md` for a device you own
   — `lsusb -v` output, VID:PID, quirks.
2. **Add a captured-device fixture.** Drop a descriptor hex dump in
   `shared/usbip-core/src/testdata/` with a round-trip test.
3. **Cover a URB edge case.** Test `RET_UNLINK` ordering, short-packet
   handling, or zero-length control transfers.
4. **Add `tracing` instrumentation** to connection setup, URB submission,
   or shutdown paths. Structured fields, useful at `info`, not noisy at `warn`.
5. **Tighten an error path** — replace a lossy `map_err(...)` with a typed
   variant. One function, one PR, one test.

See also issues labelled `good first issue` on GitHub, or open one describing
what you'd like to work on.

## Release Process

The maintainer cuts releases when milestones are met. The 1.0 release is
gated on milestones 1-10 (see `ROADMAP.md`). Pre-1.0, anything on `main` may
be tagged the same week.

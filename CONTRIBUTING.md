# Contributing to USB/IP Passthrough

Thanks for your interest in contributing. USB/IP Passthrough is a cross-platform
USB/IP bridge written primarily in Rust, with a Kotlin/Android layer and a
Windows GUI. It is a hobbyist-friendly project that also takes its engineering
seriously: we ship to gamers and home users who depend on it for their
peripherals, and we treat them accordingly.

This document covers the practical mechanics of contributing. For deeper design
context, read [`ARCHITECTURE.md`](ARCHITECTURE.md) and
[`PROTOCOL.md`](PROTOCOL.md); for milestone status, see
[`ROADMAP.md`](ROADMAP.md).

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

We expect everyone participating in this project (issues, pull requests,
discussions, comments) to be respectful and constructive. Disagreement is
welcome; rudeness is not. If you are new to open source, don't worry about
saying something wrong — the maintainers are patient. We would rather answer
ten beginner questions than have one person leave feeling unwelcome.

## Project Licensing

The project is released under the **MIT License** (see [`LICENSE`](LICENSE)).
By submitting a contribution, you agree that your work will be distributed
under the same terms. If you are contributing on behalf of an employer, make
sure you have the right to do so.

## How to Ask Questions

This is a small project. We do not run a Discord, Slack, or mailing list for
v1.0. The right place for questions is a GitHub issue with the `question`
label. Before opening one, search existing issues; your question has probably
been answered.

## Filing Bug Reports

A good bug report is reproducible. Please include:

- **OS and version** (e.g. Ubuntu 24.04, macOS 15.2, Windows 11 23H2,
  Android 14, Android TV 12).
- **USB device**: vendor and product ID (`VID:PID`), and a short description
  (e.g. `046d:c261 — Logitech G920 racing wheel`).
- **Server version** and **client version** (output of `usbip-server --version`
  and `usbip-client --version`, or the commit SHA if building from source).
- **Exact reproduction steps**: what you ran, in what order, on which machine.
- **Log excerpt**: enable verbose logging with `RUST_LOG=debug` and include the
  relevant lines. If the log is long, paste a focused excerpt around the
  failure, not the entire buffer.
- **Expected vs. actual behaviour**.

If your bug report is "the device enumerates but does not work", also include
the output of `lsusb -v` (Linux), `usbip list -l` (server side), and any
client-side errors from `dmesg` / Windows Event Viewer / `adb logcat`.

## Suggesting Features

Open a GitHub issue with the `enhancement` label. Describe the user problem
first, then the proposed solution. Tie your suggestion to a roadmap milestone
if one applies (see `ROADMAP.md`); this helps maintainers prioritise.

## Reporting Security Issues

Please do **not** open a public issue for security vulnerabilities. The
project has a [`SECURITY.md`](SECURITY.md) with a private reporting channel
(GitHub Security Advisories). Cryptographic issues in the USB/IP layer
(per `ROADMAP.md` milestone 2: AES-256-GCM, X25519 ECDH, HKDF) are the
highest-priority class of bug and will be triaged quickly.

---

## Development Setup

The repository is a Cargo workspace with four Rust crates plus Android and
Windows subprojects. The full prerequisite list lives in
[`docs/BUILDING.md`](docs/BUILDING.md); here is the short version.

### Rust (Linux, macOS, Windows)

```bash
# Install rustup if you do not have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version    # must be 1.75 or newer
cargo --version
```

On Linux, install the system USB headers:

```bash
sudo apt install build-essential pkg-config libusb-1.0-0-dev libudev-dev \
                 clang libclang-dev llvm-dev cmake
```

On Windows, install the Visual Studio 2022 Build Tools with the C++ workload
and the Windows 10/11 SDK.

### Android (optional, for the Kotlin app)

```bash
export ANDROID_HOME=$HOME/Android
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653
# Install JDK 17 and the Android command-line tools, then:
sdkmanager "platforms;android-34" "build-tools;34.0.0" "ndk;25.2.9519653"
yes | sdkmanager --licenses
rustup target add aarch64-linux-android
```

### Clone the repo

```bash
git clone https://github.com/stanvx/usb-passthrough.git
cd usb-passthrough
```

## Build and Test

Full commands and per-platform recipes are in
[`docs/BUILDING.md`](docs/BUILDING.md). The minimal checks you should run
before opening a pull request are:

```bash
# Build the entire workspace in release mode
cargo build --release

# Run the full test suite (release mode matches the CI matrix)
cargo test --release

# Run lints (recommended)
cargo clippy --all-targets -- -D warnings
```

For Android, from the `android/` directory:

```bash
./gradlew assembleDebug          # faster iteration
./gradlew :app:assembleRelease   # full release build
```

For Windows, the workspace is at `windows/`:

```bash
cargo build --release -p windows
```

If you change the wire format, regenerate any test vectors and add a
round-trip test in `shared/usbip-core/src/protocol.rs`. The CI pipeline
([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs the same
commands across Linux, Windows, and Android; please verify locally before
pushing to avoid burning CI minutes on a typo.

## Adding a USB Descriptor Fixture

The project uses captured USB descriptor trees as test fixtures. These are
used by the integration test in `shared/usbip-core/tests/descriptor_fixtures.rs`
to verify that the descriptor parser correctly handles real-world devices.

### Capturing a descriptor dump

On Linux, use `lsusb` to dump the raw descriptor tree for a device:

```bash
# Find your device's bus and device number
lsusb

# Dump the full descriptor tree (requires root for some devices)
sudo lsusb -d 046d:c261 -v 2>&1 | less

# For a raw hex dump, use usbmon or usbhid-dump:
sudo usbhid-dump -e descriptor -d 046d:c261
```

On Windows, use [USB Device Tree Viewer](https://www.uwe-sieber.de/usbtreeview_e.html)
(Uwe Sieber) or `usbview` from the Windows SDK to inspect descriptor trees.
You can also capture raw descriptors from a USB/IP server log by enabling
`RUST_LOG=debug`.

On macOS, use the `system_profiler SPUSBDataType` command or the I/O Registry:

```bash
system_profiler SPUSBDataType
ioreg -p IOUSB -l -w 0
```

### Creating a new fixture

1.  Choose a descriptive name (e.g. `logitech_g502`, `hid_keyboard`).

2.  Create the fixture directory:

    ```bash
    mkdir -p shared/usbip-core/tests/fixtures/<name>
    ```

3.  Place the raw descriptor tree bytes in `descriptor.bin`. The bytes must be
    in standard USB descriptor order (little-endian multi-byte fields):

    ```
    Device Descriptor (18 bytes)  →  type 0x01
    Configuration Descriptor (9)  →  type 0x02
        Interface Descriptor (9)  →  type 0x04
        [Class-specific descriptors, e.g. HID 0x21]
        Endpoint Descriptors (7)  →  type 0x05
    [Repeat for additional configurations]
    ```

    If you do not have a raw byte dump, you can construct one programmatically
    using the struct layout documented in `shared/usbip-core/src/descriptor.rs`.

4.  Create `metadata.toml` declaring the expected fields. Every field is
    optional; omit fields you do not want to assert:

    ```toml
    [device]
    vendor_id = 0x046d
    product_id = 0xc261
    device_class = 0
    max_packet_size0 = 64

    [config]
    num_configs = 1

    [interface]
    class = 3
    num_endpoints = 1

    [[endpoints]]
    address = 0x81
    transfer_type = "interrupt"
    ```

    For the full schema reference, see
    `shared/usbip-core/tests/fixtures/README.md`.

5.  Run the fixture test to verify:

    ```bash
    cargo test -p usbip-core --test descriptor_fixtures
    ```

    The test will parse `descriptor.bin` and assert every field present in
    `metadata.toml`. If a field does not match, the test prints the fixture
    name, the field name, the expected value, and the actual value.

6.  Commit the fixture directory:

    ```bash
    git add shared/usbip-core/tests/fixtures/<name>/
    git commit -m "test(fixtures): add <device> descriptor fixture"
    ```

## Coding Style

We optimise for code that is **boring, small, and immutable**.

- **No emojis** in source files, comments, commit messages, or documentation.
  This is a hard rule. A `docs/` page is part of the user experience and
  should read like a manual, not a chat.
- **Immutability by default**. New data, not mutation. If a function takes
  `&self` and returns a value, that is the right shape; if it takes `&mut
  self` and rewrites a field, there is almost always a better way.
- **Many small files**. Aim for 200–400 lines per file, 800 maximum. If a
  module is growing past that, split it. The workspace has 4 Rust crates
  precisely so things stay decomposable.
- **Explicit error handling**. No `unwrap()` in non-test code. No silently
  swallowed errors. Use `Result` and the existing `usbip_core::error::Error`
  type; map foreign errors into it at the boundary.
- **Validate at boundaries**. Treat anything coming off the network, USB
  bus, or filesystem as hostile. Use newtypes and parsers in
  `shared/usbip-core/src/protocol.rs` for wire-format fields.
- **Comments explain *why*, not *what***. If the code is doing something
  clever, the comment is a one-line note about the trick. If you find
  yourself writing a paragraph, the code probably wants refactoring.

For Rust-specific style, run `cargo fmt` and `cargo clippy` before pushing.
For Kotlin, follow the Android team's
[official Kotlin style guide](https://developer.android.com/kotlin/style-guide).

## Test-Driven Development

We follow strict TDD. The project enforces an **80% minimum coverage** bar;
new code is expected to come in with tests, not as a follow-up.

The project deliberately uses **inline `#[cfg(test)] mod tests` blocks** at
the bottom of each source file rather than a separate `tests/` directory.
This keeps the test next to the code it covers. When you add a public
function, add its test in the same file.

The workflow is:

1. **Write the test first** (RED). It should fail in a way that proves the
   test is actually exercising the behaviour you care about — a compile
   error from a missing function is fine; a test that passes on `unimplemented!()`
   is not.
2. **Run `cargo test -p <crate>`** and watch it fail.
3. **Write the minimum code to make it pass** (GREEN). No drive-by
   refactors, no bonus features.
4. **Run `cargo test --release`** for the full workspace.
5. **Refactor** (IMPROVE) — clean up names, extract helpers, but keep
   the tests green.
6. **Verify coverage** with `cargo tarpaulin` (Linux) or
   `cargo llvm-cov` (cross-platform). The bar is 80%.

If you are fixing a bug, start by writing a failing test that reproduces it.
This is non-negotiable: a bug fix without a regression test is not a fix.

## Commit Messages and PRs

### Branch and commit

- Branch from `main`. One topic per branch. Keep commits small and focused.
- We use **conventional commits** with the prefixes
  `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `perf:`, and
  `ci:`. The subject line should be imperative mood, 50–72 characters,
  and describe the change from the perspective of someone reading the
  diff later. Example: `fix(client): reset URB sequence number on reconnect`.
- The body, when present, explains *why*. Reference an issue number with
  `Closes #123` or `Refs #123` when applicable.
- **Sign your commits.** The maintainer's git is configured for GPG signing
  via 1Password SSH; if you have signing set up locally, the same
  configuration will work for you. Unsigned commits will not be merged.

### Pull request workflow

1. **Open a draft PR early** if you want feedback on direction. The
   maintainers would rather see a half-formed idea than no idea at all.
2. **Before requesting review**, confirm locally:
   - `cargo build --release` is clean.
   - `cargo test --release` is green.
   - `cargo clippy --all-targets -- -D warnings` has no findings.
   - `cargo fmt --check` has no diff.
3. **Describe the change**. What does this PR do, why now, and how did
   you test it? Link the issue. Include a short test plan checklist.
4. **One approval** is enough to merge for most changes. Architectural
   changes (touching the wire format, crypto, the Windows service, or
   Android service lifecycle) require a maintainer review and may take
   longer.
5. **Squash-merge** is the default. If you want to keep a multi-commit
   history intact, say so in the PR description and we can rebase-merge
   instead.

### What we will push back on

- PRs that mix formatting, refactoring, and behaviour changes. Split them.
- PRs that add a dependency without discussing it first. The dependency
  surface matters; explain why the new crate is the right call.
- PRs that touch crypto without an accompanying test vector. We treat
  AES-256-GCM and X25519 code as security-sensitive; every change needs
  coverage.

---

## Areas We Especially Want Help With

These are the highest-leverage places to spend your time. The list is
informed by `ROADMAP.md` and the current state of the workspace.

1. **macOS server and client** (roadmap milestone 12). The macOS I/O Kit
   enumeration, IOUSBFamily integration, and a uinput-equivalent input
   path are all open. Touching platform code is the deepest kind of
   contribution and very welcome.
2. **Non-Logitech device compatibility**. Milestone 4 mentions a
   `VID:PID` allowlist; the G920 is the most-tested device, but anything
   in the HID, mass-storage, or USB-to-serial class is in scope. See
   "Good First Issues" below for the lightweight version of this.
3. **Performance profiling on Raspberry Pi and other low-end ARM boards**.
   The roadmap (milestone 11) lists URB buffer pool tuning, zero-copy
   forwarding, and batched URB submission — all things that matter more
   on a Pi than on a desktop. If you have a Pi and a USB device, you have
   the lab.
4. **Documentation translation**. The project ships English docs
   (`docs/`, `ARCHITECTURE.md`, `PROTOCOL.md`, `README.md`); translations
   into other languages are entirely community-driven and very much
   wanted. Open a `docs/i18n/<lang>/` directory and add a
   `TRANSLATING.md` so the next translator knows where to pick up.
5. **Test fixtures for non-Logitech devices**. We have very few captured
   USB descriptors in-tree. Even a single hex dump from a real device,
   with the device class, VID:PID, and a one-line note about how it
   behaves, is gold for the test suite.

Other recurring needs: hot-plug support, USB isochronous transfers
(audio/video), IPv6, and the Prometheus-style metrics endpoint
(milestones 11 and 13). Pick what interests you.

## Good First Issues

These are concrete, scoped tasks suitable for a first or second
contribution. They are also genuinely useful — they are not "make-work"
items to fill a label.

1. **Add a non-Logitech device to the docs.** `docs/G920-SPECIFIC.md`
   is the G920's deep dive; add a sibling `docs/DEVICES.md` (or a
   short section in an existing page) describing a different device you
   own — a USB headset, a gamepad, a USB-to-serial adapter. Include
   `lsusb -v` output, the `VID:PID`, and any quirks (e.g. "needs
   endpoint 0x82 in bulk mode").
2. **Add a captured-device fixture.** Drop a real device descriptor
   (hex bytes or a parsed `DeviceDescriptor` struct) into
   `shared/usbip-core/src/testdata/` and add a `#[test]` that
   round-trips it through the parser. Even one fixture per device class
   helps.
3. **Cover a known URB edge case.** The protocol has a few sharp edges
   around `RET_UNLINK` ordering, short-packet handling on bulk endpoints,
   and zero-length control transfers. Pick one and add a test in
   `shared/usbip-core/src/urb.rs` that exercises it with a synthetic
   `UsbIpHeader`.
4. **Add `tracing` instrumentation to a hot path.** Choose one of the
   connection setup, URB submission, or shutdown paths and add a few
   `tracing::debug!` or `tracing::info!` calls with structured fields
   (bus ID, sequence number, peer address). Make sure the messages are
   useful at `RUST_LOG=info` and not noisy at `warn`.
5. **Tighten a single error path.** Find a function that returns
   `Result<_, Error>` but loses context (`map_err(|e| Error::Other(e.into()))`),
   and replace it with a typed variant that carries the failing field or
   operation. One function, one PR, one test.

If none of these match what you want to do, look at issues labelled
`good first issue` on GitHub, or open an issue describing what you'd like
to work on — we will help you scope it.

## Release Process

The maintainer cuts a release when milestones are met. The current
versioning scheme is documented in `ROADMAP.md`; the 1.0.0 release is
gated on milestones 1–10 being closed. Pre-1.0, anything that lands on
`main` is fair game for a tagged release the same week.

---

Welcome aboard. The best contributions start with a small PR that closes
an existing issue — open one today.

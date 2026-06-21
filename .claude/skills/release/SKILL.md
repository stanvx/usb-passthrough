---
name: release
description: Cut a new release — version bump, changelog, pre-flight checks, tag, and push. Use when asked to release, ship, or publish a new version.
disable-model-invocation: true
---

## Release Workflow

The release is tag-driven — pushing a `v*` tag triggers `.github/workflows/release.yml` which builds Windows EXE + Android APK and publishes a GitHub Release.

### Pre-flight Checks

Run these BEFORE bumping the version. **On non-Windows hosts, exclude the `anyplug` (Windows) crate** — it has Windows-only FFI bindings that don't compile on Linux. This mirrors the project's CI behavior (see CLAUDE.md "Gotchas Claude gets wrong without this").

```bash
# Detect host platform
case "$(uname -s)" in
  MINGW*|CYGWIN*|MSYS*) ON_WINDOWS=true ;;
  *) ON_WINDOWS=false ;;
esac
case "${OS:-}" in
  Windows_NT) ON_WINDOWS=true ;;
esac

# On non-Windows hosts, exclude the `anyplug` (Windows) crate
WORKSPACE_EXCLUDES=()
if [ "$ON_WINDOWS" = false ]; then
  WORKSPACE_EXCLUDES=(--exclude anyplug)
fi

# 1. Full workspace check
cargo check --workspace "${WORKSPACE_EXCLUDES[@]}"

# 2. All tests
cargo test --release --workspace "${WORKSPACE_EXCLUDES[@]}"

# 3. Strict lint (treat warnings as errors)
cargo clippy --workspace "${WORKSPACE_EXCLUDES[@]}" -- -D warnings

# 4. Format check
cargo fmt --all -- --check
```

If any step fails, fix before proceeding.

**On non-Windows hosts:** the `anyplug` (Windows) crate is excluded from the pre-flight. The Windows EXE build is verified in the GitHub Actions release workflow (`windows-latest` runner, MSVC), NOT in local pre-flight. See "Known Caveats" below.

### Version Bump

The workspace version lives in root `Cargo.toml` — all crates use `version.workspace = true` (except `windows/Cargo.toml` which has its own `version = "0.3"` — update both).

1. Update root `Cargo.toml`: `version = "X.Y.Z"`
2. Update `windows/Cargo.toml`: `version = "X.Y"` (if it tracks the workspace major.minor)
3. Verify with `cargo check --workspace` (catches version mismatch in Cargo.lock)

### Changelog

Create or update `CHANGELOG.md` at the repo root. Use [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...
```

Source the entries from `git log v<previous>..HEAD --oneline`.

### Tag & Push

```bash
# Commit version bump + changelog
git add Cargo.toml windows/Cargo.toml CHANGELOG.md
git commit -m "chore: bump version to X.Y.Z"

# Create and push the tag
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin main
git push origin vX.Y.Z
```

### Post-tag Monitoring

The release workflow builds:
- **Windows EXE** (windows-latest, MSVC, `cargo build --release -p anyplug`)
- **Android APK** (ubuntu-latest, JDK 17, `gradle assembleRelease`)

Both steps use `continue-on-error: true` — check the Actions run even if it appears green. Verify the artifacts are actually present in the GitHub Release.

### Known Caveats

- **Windows build** (`continue-on-error: true`): a failed build still publishes an empty release
- **Android build** (`continue-on-error: true`): requires the `android/rust/usbip-android/` JNI crate which may not be committed
- No Linux binary is built by the release workflow — Linux users build from source (`cargo build --release`)

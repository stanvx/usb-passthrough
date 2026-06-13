---
name: verify
description: Pre-commit verification gate — runs check, test, clippy, and format checks across the Rust workspace. Use before committing or when asked to verify changes.
disable-model-invocation: true
---

## Steps

Run these commands in order, stopping on first failure:

```bash
# 1. Fast compilation check
cargo check --workspace

# 2. All tests
cargo test --release --workspace

# 3. Strict linting
cargo clippy --workspace -- -D warnings

# 4. Format check
cargo fmt --all -- --check
```

If any command fails, report the failure with the exact output and do NOT proceed to the next step.
If all pass, confirm: "All checks passed — ready to commit."

---
description: Install latest branch-tone binary after code changes
---
# /install — Install Latest Binary

After making changes to `src/main.rs`, install the updated binary so the hooks use the latest code.

## Steps

1. Run tests first to ensure nothing is broken:
   ```
   cargo test
   ```
2. Install the binary:
   ```
   cargo install --path .
   ```
   Note: This writes to `~/.cargo/bin/` which requires disabling the sandbox.

3. Verify:
   ```
   branch-tone --version
   ```

## When to Run

- After any code change to `src/main.rs`
- Before committing (ensures the binary matches the source)
- After `cargo test` passes with zero warnings

## Automatic Reminder

**After modifying `src/main.rs` and confirming tests pass, always offer to install the latest binary.** The hooks call the installed binary, not the dev build — stale installs mean the user hears old sounds.

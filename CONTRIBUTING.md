# Contributing to BentoDesk

Bug reports, fixes, translations, themes, plugins, and documentation are welcome. Chinese and English are both fine.

## Before opening a pull request

1. Search existing issues and discussions.
2. Keep the change focused; do not mix unrelated cleanup into a fix.
3. Test against real Windows behavior. Use an isolated `BENTODESK_STATE_DIR` instead of your normal application data.
4. Describe what you verified and what you could not verify.

The usual local checks are:

```powershell
$env:CARGO_BUILD_JOBS = "1"
$env:CARGO_INCREMENTAL = "0"

cargo fmt --all -- --check
cargo test --workspace --all-targets --target x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo build --release --target x86_64-pc-windows-msvc -p bentodesk-shell --bin BentoDesk
```

Changes touching file moves, plugins, updates, settings, or recovery must preserve data on failure and explain the rollback path.

## Project boundaries

- The product name is `BentoDesk`; Rust packages use the `bentodesk-*` prefix.
- The 2.x runtime is native Rust and Win32. Do not add a browser runtime or a second desktop framework.
- Keep machine-local paths, credentials, runtime state, logs, and captured user data out of commits.
- New dependencies need a concrete reason and must pass `cargo deny check` and `cargo audit`.

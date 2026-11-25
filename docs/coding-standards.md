# Coding standards

- Rust 2021 or newer.
- `cargo clippy --all-targets -- -D warnings` is clean.
- Library code does not call `unwrap` outside tests.
- `mnemic-core` returns errors with `thiserror`.
- Every commit is a Conventional Commit.
- Pull request titles are `SYL-N: summary`. A pull request title is never a Conventional Commit.

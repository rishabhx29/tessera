## What and why

Closes #(issue)

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test --all` passes
- [ ] Docs updated (README / `docs/` / ADRs) where the change alters documented behavior
- [ ] No performance numbers added without measured hardware and methodology
- [ ] No new dependencies without a note on why (size, license, maintenance)

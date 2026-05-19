# Contributing to Nusa Runtime

Thank you for your interest in contributing! This project follows a strict meta-cognitive framework to ensure architectural integrity.

## Meta-Cognition Framework
All contributions must trace through three layers:
1. **Layer 3 (Domain/WHY):** Does this solve a real Laravel/Cloud-Native problem?
2. **Layer 2 (Design/WHAT):** Is the architecture sound, modular, and safe?
3. **Layer 1 (Mechanics/HOW):** Does it use Rust idioms correctly (ownership, concurrency, errors)?

## RFC Process
Major changes require an RFC.
1. Copy `rfcs/0000-template.md` to `rfcs/00XX-my-feature.md`.
2. Fill out the template, focusing on **Motivation** and **Detailed Design**.
3. Submit a PR with `[RFC]` prefix.
4. Discussion period: 1 week.
5. Merge if approved by maintainers.

## Code Standards
- **Safety:** `#![deny(unsafe_code)]` is mandatory outside `phprt-engine-ffi`.
- **Errors:** Use `thiserror` for domain errors. No `unwrap()` in library code.
- **Naming:** No `get_` prefix. snake_case (fn/var), CamelCase (type), SCREAMING_CASE (const).
- **Formatting:** Run `cargo fmt` before committing.
- **Linting:** Run `cargo clippy -- -D warnings` before committing.
- **Concurrency:** Don't hold locks across `.await`. Use `parking_lot::Mutex`, not `std::sync::Mutex`.

## Reporting Issues
Use the provided issue templates. Include:
- Rust version (`rustc --version`)
- OS/Docker environment
- Minimal reproducible example
- Logs (with `RUST_LOG=debug`)

## Security Vulnerabilities
Please report security vulnerabilities to `security@phprt.dev`. Do not open public issues. See `SECURITY.md` for details.

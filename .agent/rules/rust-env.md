---
trigger: always_on
---

# Rust Environment Management Rules

## 1. Detection

- **Check for Cargo Workspace**: Always check for a `Cargo.toml` at the project root before running any command. For multi-crate projects, check for a `[workspace]` section to understand the full project layout.
- **Check for Project Docs**: Always check for a README.md and DESIGN.md for a high-level overview of the project.

## 2. Execution

- **No Global Tool Assumptions**: Do not assume globally installed versions of `rustc`, `cargo`, or other tools match project requirements.
- **Use Cargo for All Tasks**:
  - **Build**: Use `cargo build` (debug) or `cargo build --release` (release).
  - **Run**: Use `cargo run` or `cargo run --bin <binary_name>` for projects with multiple binaries.
  - **Test**: Use `cargo test` — never invoke test binaries directly.
  - **Lint**: Use `cargo clippy -- -D warnings` to treat all warnings as errors.
  - **Format**: Use `cargo fmt` to format code before presenting work.
  - **Other Tools**: Locate project-local tools (e.g., `cargo-nextest`, `cargo-audit`) via `cargo install` or check for presence in the workspace before use.
- **Dependency Management**: Use `Cargo.toml` for all dependencies under `[dependencies]`, `[dev-dependencies]`, and `[build-dependencies]`. Never manually edit `Cargo.lock` — let Cargo manage it.
- **Linting & Formatting**: Always run `cargo fmt` and `cargo clippy -- -D warnings` before presenting work.
- **Version Control**: Never automatically commit to Git for the user unless explicitly told.

## 3. Environment Constraint

- **Toolchain Pinning**: If a `rust-toolchain.toml` or `rust-toolchain` file exists at the project root, all `cargo` and `rustc` invocations must respect it. Never override or ignore the pinned toolchain.
- **No Global Overrides**: Never use `rustup override set` or pass `+<toolchain>` flags unless explicitly instructed.
- **No Direct `rustc` Invocation**: Always go through `cargo` — never invoke `rustc` directly for project compilation.

## 4. Project Structure Execution

- **Binary Projects**: The project uses a standard `src/` structure. To run the main binary, use `cargo run` or the compiled artifact in `target/debug/<name>` or `target/release/<name>` rather than invoking source files directly.
- **Library Projects**: Run tests and examples via `cargo test` and `cargo run --example <name>` respectively.
- **Workspace Projects**: Always specify the target crate with `-p <crate_name>` when a command should be scoped to a single member, e.g., `cargo test -p my_crate`.

## 5. Agent Knowledge

### Reading Learnings

Before starting any task, read `.agent/rules/rust-learning.md` (create if missing). This file contains accumulated learnings specific to Rust work in this codebase. Apply any relevant entries to inform your approach before writing a single line of code.

### Recording Learnings

Your learning file is **NOT a work log** — do not record routine progress or successful changes without surprises. Only add an entry when you discover something that would meaningfully change how you or another agent approaches this codebase in the future.

**✅ ONLY add entries when you discover:**

- A borrow checker or lifetime pattern that was unexpectedly required by this codebase's architecture
- A Cargo feature, flag, or toolchain behavior that behaved differently than expected
- A refactor or optimization that was rejected, and the reason why
- A codebase-specific Rust pattern or anti-pattern (e.g. how errors are structured, how async is wired)
- A surprising edge case in how this project handles traits, generics, or unsafe code

**❌ DO NOT record:**

- "Implemented feature X" (unless there's a non-obvious learning)
- Generic Rust tips that apply to any project
- Successful changes that went exactly as expected

**Format:**

```markdown
## YYYY-MM-DD - [Title]
**Learning:** [What you discovered and why it matters]
**Action:** [How to apply this next time]
```

### File Layout

Learnings are separated by language, one file per language, all under `.agent/learning/`:

- Rust → `.agent/learning/rust.md`
- Python → `.agent/learning/python.md`
- TypeScript/JavaScript → `.agent/learning/typescript.md`
- (Add new files for other languages as needed)

## 6. General Development Rules

- **Documentation Updates**: Every change should update inline doc comments (`///`) for all public items and update the `README.md` appropriately.
- **Docstrings**: Every new or modified public function, struct, enum, and trait must have a `///` doc comment. Include an `# Examples` section where applicable.
- **Testing**: Add unit tests in an inline `#[cfg(test)]` module for all new functions. Add integration tests under `tests/` for new public API surface.
- **Error Handling**: Use `Result` and `?` propagation — avoid `unwrap()` and `expect()` outside of tests and examples unless explicitly justified with a comment.
- **Comments**: Line comments should be used to explain key lines and sections of code, such as loops or function calls.

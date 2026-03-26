# Agent guide

## Project overview

`escli` is a CLI for Elasticsearch generated from the [elasticsearch-specification](https://github.com/elastic/elasticsearch-specification) schema. The codebase is a Rust workspace with three crates:

- **`generator`** — reads `schema.json` and writes all of `escli/src/`
- **`escli`** — the CLI binary; its `src/` is entirely generated (gitignored)
- **`escli/staticcmds`** — hand-written commands that don't come from the schema (e.g. `info`)

## Critical workflow rule

**`escli/src/` is gitignored.** The generator must be run before `escli` can be built or tested:

```
cargo run -p generator          # debug (fast compile, fine for local work)
cargo run -p generator --profile release  # used in CI
cargo test -p escli             # requires generated files to be present
```

Changes to generator templates (`generator/src/`) do not take effect until regeneration. Always regenerate and rerun tests after touching the generator.

## Schema caching

The generator caches the downloaded schema as `schema-{branch}.json` so it doesn't re-download on every run. `schema*.json` is gitignored. On a fresh checkout the schema is downloaded automatically.

## PR and commit conventions

- **Always create a PR branch and open a PR** — never push directly to `main`, even for small follow-up commits
- Branch naming: `pr/<short-description>` (e.g. `pr/better-errors`)
- Use `gh pr create --base main` to open PRs
- Stacked PRs: after each merge, rebase the next branch onto `main` and retarget with `gh pr edit --base main`

## CI

`.github/workflows/build.yml` has three jobs:

| Job | Runs on | When | Purpose |
|---|---|---|---|
| `test` | ubuntu-latest | every PR and push | Gate: debug build, runs all tests |
| `generate` | ubuntu-latest | push to main only | Builds generator in release, uploads `escli/src/` as artifact |
| `build` | ubuntu + macos + windows | push to main only | Downloads generated source, builds release binary, uploads artifact |

The test job runs the generator in **debug** mode (faster compile; the generator runs in milliseconds regardless of optimization). The build job builds escli in **release** mode for distribution.

## Testing

Integration tests live in `escli/tests/cli.rs` and use:
- **`assert_cmd`** — runs the compiled `escli` binary as a subprocess
- **`wiremock`** — async HTTP mock server
- **`tempfile`** — for `.env` file tests

Key patterns:
```rust
// Helper that pre-wires --url for every test
fn escli(server: &MockServer) -> Command {
    let mut cmd = Command::cargo_bin("escli").unwrap();
    cmd.args(["--url", &server.uri()]);
    cmd
}

// Most tests: mount a mock, run the CLI, verify
Mock::given(method("GET")).and(path("/")).respond_with(...).mount(&server).await;
escli(&server).arg("info").assert().success();
server.verify().await;  // asserts mock was called the expected number of times
```

Platform-specific tests use `#[cfg(unix)]` / `#[cfg(windows)]`.

## Error handling conventions

All error paths in `main()` must:
1. Write the message to **`stderr`** using the `stderr` variable (not `tokio::io::stderr()` inline)
2. Call **`stderr.flush().await.ok()`** before `std::process::exit(1)` — without this the buffer is silently discarded
3. Never propagate errors via `?` to the tokio main wrapper — that would print via `Debug` instead of `Display`

The `EscliError` enum has four variants: `Transport`, `Command`, `Execution`, `Io`. `From<elasticsearch::Error>` produces friendly messages for the common cases:
- Connection refused/DNS → `"Could not connect to <url>: <root cause>"`
- Timeout → `"Request timed out — try increasing --timeout"`
- Other → `"Request failed: <message>"`

## Generator code structure

| File | Purpose |
|---|---|
| `generator/src/main.rs` | Entry point; orchestrates schema download, endpoint parsing, file writing |
| `generator/src/endpoint.rs` | Generates per-endpoint command structs and `Executor` impls |
| `generator/src/enumeration.rs` | Generates enum types; stores `(wire_name, code_name)` pairs to handle dot-containing member names (e.g. `logs.otel`) |
| `generator/src/field.rs` | Field type resolution; `Vec<_>` fields are forced to `String` in path params |
| `generator/src/cli.rs` | Generates `escli/src/main.rs` — auth setup, dispatch, response routing |
| `generator/src/esclierror.rs` | Generates `escli/src/error.rs` — `EscliError` enum and `From` impls |
| `generator/src/cmd.rs` | Generates `escli/src/cmd.rs` — `BTreeMap`-ordered dispatch table |
| `generator/src/namespace.rs` | Generates per-namespace module headers |

Code generation uses **`genco`** (`quote!` macro). String formatting helpers like `convert_case` are used for PascalCase conversion — note that `.` is not treated as a word separator, which is why `codegen_name` overrides exist in the schema.

## Excluded endpoints

`generator/src/main.rs` has `EXCLUDED_ENDPOINTS` and `EXCLUDED_PREFIXES` constants that skip endpoints which don't map cleanly to the CLI (e.g. endpoints with no fixed path structure).

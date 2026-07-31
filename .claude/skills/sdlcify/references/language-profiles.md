# Language profiles

Read the section for whichever marker file Phase 0 found. Each profile
gives the `.gitignore` lines to ensure present and a CI job skeleton to
use *only* when no CI workflow exists yet.

Universal lines, added regardless of language (OS/editor cruft that has
nothing to do with the toolchain):

```
.DS_Store
*.swp
*.swo
*~
```

## Rust (`Cargo.toml`)

`.gitignore` additions:

```
/target
```

`Cargo.lock`: **do not ignore it.** Ignore it only if the crate is a
pure library with no `[[bin]]` target and no examples meant to be run
standalone — check `Cargo.toml` for `[[bin]]` entries or a `src/main.rs`
before deciding; when in doubt (mixed lib+bin, like a crate that ships
both a library and a CLI), leave it tracked. An untracked lockfile on an
application crate means CI and contributors can silently drift onto
different dependency versions.

CI skeleton (only if `.github/workflows/ci.yml`-equivalent is absent):

```yaml
name: ci
on:
  push:
    branches: ["main"]
  pull_request:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo build
      - run: cargo test
```

(Use `macos-latest` instead of `ubuntu-latest` if the repo has
platform-specific rendering/UI code that needs it — check the existing
workflow's runner if one exists elsewhere, e.g. a release workflow.)

## Node / TypeScript (`package.json`)

`.gitignore` additions:

```
node_modules/
dist/
.env
.env.local
```

CI skeleton:

```yaml
name: ci
on:
  push:
    branches: ["main"]
  pull_request:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "lts/*"
          cache: "npm"
      - run: npm ci
      - run: npm run build --if-present
      - run: npm run lint --if-present
      - run: npm test --if-present
```

## Python (`pyproject.toml` / `requirements.txt`)

`.gitignore` additions:

```
__pycache__/
*.pyc
.venv/
dist/
*.egg-info/
```

CI skeleton (assumes `ruff` for lint+format; adjust if the repo already
uses `flake8`/`black` — check `pyproject.toml`'s `[tool.*]` sections
before assuming):

```yaml
name: ci
on:
  push:
    branches: ["main"]
  pull_request:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.x"
      - run: pip install -e .[dev]
      - run: ruff check .
      - run: ruff format --check .
      - run: pytest
```

## Go (`go.mod`)

`.gitignore` additions:

```
/bin
*.exe
```

CI skeleton:

```yaml
name: ci
on:
  push:
    branches: ["main"]
  pull_request:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version: "stable"
      - run: go build ./...
      - run: go vet ./...
      - run: test -z "$(gofmt -l .)"
      - run: go test ./...
```

# hex-lint

A Cargo workspace lint that enforces hexagonal-architecture role boundaries. You tag each workspace member with a role (`domain`, `usecase`, `port-and-adapter`, `driven-adapter`, `driving-adapter`, `infra`, `composition-root`) and `hex-lint` walks your workspace dependency graph and fails the build on any cross-role edge that the matrix forbids. Grandfathered debt is recorded in an exceptions file; **stale exceptions also fail the lint**, so the file is forced to stay honest as debt is paid off.

This is opinionated. The role matrix is hardcoded. If you want a different matrix, fork it.

## Install

From git (until this lands on crates.io):

```sh
cargo install --locked --git https://github.com/igouss/hex-lint hex-lint
```

## Tag your crates

In every workspace member's `Cargo.toml`:

```toml
[package.metadata.hex-arch]
role = "domain"   # or usecase, port-and-adapter, driven-adapter,
                  # driving-adapter, infra, composition-root
```

A workspace member without a role is a hard error.

## Run

From the workspace root:

```sh
hex-lint
```

Exits 0 on clean, non-zero on any violation. Output:

```
hex-lint: clean (42 workspace packages, 0 active violation(s) all sanctioned by 0 exception(s))
```

## The matrix

Each row says: *a crate of this role may depend on crates of these roles, and no others.* Workspace-internal `[dependencies]` and `[build-dependencies]` are checked. `[dev-dependencies]` are not — tests get to wire whatever they need.

| Consumer            | May depend on |
| ------------------- | ------------- |
| `domain`            | `domain` |
| `usecase`           | `domain`, `usecase`, `port-and-adapter` |
| `port-and-adapter`  | `domain`, `port-and-adapter` |
| `driven-adapter`    | `domain`, `port-and-adapter`, `infra` |
| `driving-adapter`   | `domain`, `usecase`, `port-and-adapter` |
| `infra`             | `infra` |
| `composition-root`  | everything (this is where wiring lives) |

The point of the matrix:

- **`domain`** is the pure heart. No outward dependencies. No frameworks, no I/O, no async runtimes. Just types and rules.
- **`usecase`** orchestrates application behavior. Talks to the outside world only through `port-and-adapter` traits.
- **`port-and-adapter`** holds the trait definitions (the *ports*) and the domain types they speak in.
- **`driven-adapter`** implements ports against real infrastructure (DB, HTTP client, filesystem). Imports `infra` for plumbing.
- **`driving-adapter`** is what calls in: HTTP servers, CLIs, TUIs. They hold a usecase, take input, render output.
- **`infra`** is framework / runtime / glue (logging, config loading, error types). May only depend on other `infra`. No domain knowledge.
- **`composition-root`** is the binary or top-level crate that wires concrete adapters into usecases. The only place the full graph is allowed.

If your code doesn't fit, your code is wrong, or your matrix is wrong. Pick one.

## Exceptions

Real codebases have grandfathered debt. Record it in `hex-lint-exceptions.toml` at the workspace root:

```toml
[[exception]]
consumer = "my-usecase-crate"
dep = "my-driven-adapter-crate"
ticket = "JIRA-1234"
reason = "Will be cleaned up when we extract the port — see ticket."
```

`ticket` and `reason` are documentation only — the lint doesn't read them. They're for the next person who opens this file in six months.

The file location can be overridden with `--exceptions <PATH>`. If no `--exceptions` flag is given and the default file is missing, hex-lint runs with zero exceptions (which is the right default for a clean codebase).

**Stale exceptions fail the lint.** If you list a violation that no longer exists, hex-lint complains and exits non-zero. This is intentional — it means you can't paper over architectural debt and forget about it. The file rots loud.

## Pre-commit hook

```sh
#!/bin/sh
hex-lint || exit 1
```

Or wire it into a `justfile`:

```just
hex-lint:
    hex-lint

install-hex-lint:
    cargo install --locked --git https://github.com/igouss/hex-lint hex-lint
```

## Options

```
hex-lint [OPTIONS]

OPTIONS:
    -e, --exceptions <PATH>    Exceptions TOML. Default: <workspace-root>/hex-lint-exceptions.toml
        --manifest-path <PATH> Path to a Cargo.toml in the workspace.
    -h, --help                 Print help.
    -V, --version              Print version.
```

## Why

Because "we'll keep dependencies clean" is a lie that a sufficiently large team tells itself for about six months. The matrix is mechanical. The build either passes or it doesn't. There is no debate.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

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
role = "domain"   # or kernel, usecase, port-and-adapter, driven-adapter,
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
| `kernel`            | `kernel` |
| `domain`            | `kernel`, `domain` |
| `usecase`           | `kernel`, `domain`, `usecase`, `port-and-adapter` |
| `port-and-adapter`  | `kernel`, `domain`, `port-and-adapter` |
| `driven-adapter`    | `kernel`, `domain`, `port-and-adapter`, `infra` |
| `driving-adapter`   | `kernel`, `domain`, `usecase`, `port-and-adapter` |
| `infra`             | `infra` |
| `composition-root`  | everything (this is where wiring lives) |

The point of the matrix:

- **`kernel`** is the floor: the shared vocabulary every layer above speaks — the traits and types that would otherwise be duplicated into each domain crate. It may not depend on `domain`, which is what separates it from "a domain crate everyone happens to import". Optional; most workspaces never need one.
- **`domain`** is the pure heart. No outward dependencies. No frameworks, no I/O, no async runtimes. Just types and rules.
- **`usecase`** orchestrates application behavior. Talks to the outside world only through `port-and-adapter` traits.
- **`port-and-adapter`** holds the trait definitions (the *ports*) and the domain types they speak in.
- **`driven-adapter`** implements ports against real infrastructure (DB, HTTP client, filesystem). Imports `infra` for plumbing.
- **`driving-adapter`** is what calls in: HTTP servers, CLIs, TUIs. They hold a usecase, take input, render output.
- **`infra`** is framework / runtime / glue (logging, config loading, error types). May only depend on other `infra`. No domain knowledge.
- **`composition-root`** is the binary or top-level crate that wires concrete adapters into usecases. The only place the full graph is allowed.

If your code doesn't fit, your code is wrong, or your matrix is wrong. Pick one.

### Direct edges are enough

`hex-lint` checks only *direct* workspace dependencies, and that is not a shortcut. The matrix is **transitively closed**: for every role R and every role A that R may depend on, everything A may depend on, R may depend on too. So a dependency *closure* can never reach a role the direct matrix forbids — if every direct edge passes, every transitive edge passes for free.

`role::tests::matrix_is_transitively_closed` proves this and fails on any future row that breaks it. If you maintain a separate transitive dep-graph walk alongside `hex-lint`, it is enforcing nothing extra and is free to drift out of agreement — delete it.

The one hole is deliberate: a sanctioned exception admits an illegal *direct* edge, and every consumer of that crate inherits it transitively. Closure is a property of the matrix, not of a workspace that has overridden it — which is the real reason stale exceptions fail the lint.

## Scope: crate granularity

hex-lint enforces roles at **Cargo workspace-member (crate) granularity**, and that is a deliberate choice, not a missing feature. Each member carries one role; hex-lint tags it, walks the crate dependency graph, and fails any cross-role edge the matrix forbids.

What this means in practice:

- **One role per crate.** If you want a boundary enforced, the two sides have to live in different crates. The crate boundary is the only boundary Rust actually enforces — it's the compilation unit, the visibility wall, and the edge Cargo tracks. A tool that respects it can give you a hard guarantee.
- **Intra-crate mixing is *not* checked.** A `domain` module importing an `infra` module *inside the same crate* passes clean. hex-lint reads the Cargo dependency graph, not your module tree — it does not parse source. If your `domain` and `infra` code share a crate, they can `use` each other freely and nothing here will stop them.

So if you tag your crates and hex-lint says **clean**, the guarantee is precise: *no forbidden dependency exists between your crates.* It is **not** a claim that the layering inside any single crate is sound. The fix for inside-a-crate layering is to split the crate along the role boundary you care about — then hex-lint enforces it for free. For module-level discipline within one crate, that's clippy/rustc visibility territory, not this tool.

## Context isolation

The matrix proves your dependency arrows respect *layer* direction. It is blind to *bounded contexts*: a `driving-adapter` in your `shopping` context may, per matrix, legally depend on a `usecase` in your `pantry` context — legal by layer, forbidden by architecture. Context isolation is a second, orthogonal check over the same edge set that closes that gap. It is off until you ask for it.

Tag a crate with a context alongside its role:

```toml
[package.metadata.hex-arch]
role    = "usecase"
context = "shopping"
```

**The rule is one total predicate.** An edge `consumer → dep` passes iff `consumer.context == dep.context` **or** `dep.context == "shared"`. That single line already subsumes "a shared crate may depend only on shared": a `shared` consumer passes only when its dependency is also `shared`. `"shared"` is the one reserved context name — any crate may depend on it, and it may depend only on itself. Every other name is free-form; call your contexts whatever your domain calls them.

**It is opt-in, and it is all-or-nothing.** Declare zero contexts and the axis is off — the run is byte-for-byte what it was before, roles only. Declare a context on *every* workspace member and the axis is on. Declare one on *some but not all* members and hex-lint refuses to run: **partial adoption is a hard error** that names the crates missing a context and exits non-zero before any check runs. This mirrors roles, where a member without a role is already a hard error.

**Exceptions are the same file and the same schema.** Grandfathered cross-context debt goes in `hex-lint-exceptions.toml` next to your role debt, tagged with `axis = "context"`:

```toml
[[exception]]
consumer = "shopping-reactor"
dep      = "pantry-shell"
axis     = "context"
ticket   = "ARCH-42"
reason   = "temporary reach-in until the shared contract is extracted"
```

`axis` defaults to `"role"` when omitted, so every exception file written before this feature keeps parsing unchanged. Each check reads only its own axis: a `role` exception never suppresses a context violation, and vice versa. And exactly as with roles, a context exception that no longer matches a real violation is **stale** and fails the lint. The file rots loud on both axes.

**What a green run certifies — precisely.** hex-lint enumerates the *full* `cargo metadata` dependency graph, and both predicates are **total**: role-matrix membership is defined for every (consumer, dep) role pair, and context equality-or-`shared` is defined for every context pair. There is no sampling, no heuristic, no approximation in either direction. So when hex-lint says clean, then over the crates *as you tagged them* no forbidden edge exists, on either axis. The one thing it cannot check is whether you tagged them correctly: a crate wearing the wrong `context` is a **user bug, not a tool bug**. That is the whole claim, and no more.

**One named limitation.** Context isolation is enforced at the **crate-dependency level only** — the same granularity as roles, for the same reason: the crate is the only boundary Cargo tracks. A foreign type copy-pasted *inside* a crate, or two contexts sharing one crate and `use`-ing each other freely, is out of scope. hex-lint reads the dependency graph, not your source. If you want a context boundary enforced, the two sides have to live in different crates.

## Explain a role

When a violation fires, hex-lint prints the broken rule and concrete fixes inline. For the full contract of any role on demand:

```sh
hex-lint explain usecase
```

```
hex-lint — role `usecase`

May depend on: domain, usecase, port-and-adapter

Contract:
    a usecase orchestrates application behavior and may reach the outside world
    only through ports — never an adapter or infra crate directly.

If hex-lint flags a forbidden dependency out of a `usecase` crate:
  - Declare the capability you need as a port (trait) in a port-and-adapter crate and depend on that; a driven-adapter implements it.
  - Let the composition-root inject the concrete implementation — the usecase only ever names the trait.
```

The same guidance rides along on every violation in `--format=json` (under each violation's `remediation` key), so an agent fixing the build gets the recovery path, not just the failing edge.

## Exceptions

Real codebases have grandfathered debt. Record it in `hex-lint-exceptions.toml` at the workspace root:

```toml
[[exception]]
consumer = "my-usecase-crate"
dep = "my-driven-adapter-crate"
axis = "role"   # optional: "role" (the default) or "context"
ticket = "JIRA-1234"
reason = "Will be cleaned up when we extract the port — see ticket."
```

`axis` selects which check the exception belongs to and defaults to `"role"`, so this same file holds both role and context debt (see [Context isolation](#context-isolation)). `ticket` and `reason` are documentation only — the lint doesn't read them. They're for the next person who opens this file in six months.

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
hex-lint explain <ROLE>        Print a role's contract and how to fix violations.

OPTIONS:
    -e, --exceptions <PATH>    Exceptions TOML. Default: <workspace-root>/hex-lint-exceptions.toml
        --manifest-path <PATH> Path to a Cargo.toml in the workspace.
    -f, --format <FMT>         Output format: text (default) or json.
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

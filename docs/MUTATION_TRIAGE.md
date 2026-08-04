# Mutation triage: the survivor loop

A surviving mutant is one of exactly two things:

- an **equivalent mutant** — semantically identical to the original, therefore
  unkillable, genuine noise; or
- a **test gap** — real, and the whole point of running mutation testing.

Without a persistent record of adjudicated equivalents, every re-sweep
re-presents the same noise and the loop never converges. This document is the
convention that makes it converge, plus the two rules that stop it converging
on a lie.

Everything here is enforced by tooling, not by good intentions: `just
mutants-gates` is blocking inside `just verify`. (`just mutant-schema-neutral`
is blocking too, but only in this tool's own repo — it guards the neutral core
against learning a generator's vocabulary, and a project that installs the
binary has no such core to violate.)

---

## 1. The loop

```
just mutants-preflight <crate>     # seconds — refuse a sweep whose tests never run
just mutants -p <crate>            # hours — the sweep itself, serially, off-hours
just mutants-triage <crate>        # the classified report; the ONLY thing an agent reads
    ├── equivalent → just mutants-admit ...   (gated on two refutations)
    └── test gap   → just mutants-bead-draft ...  then a bead, then a test,
                     then just mutants-kill  ← the only thing that closes it
```

Exit codes from `just mutants-triage`:

| code | meaning |
|---|---|
| `0` | converged — no new survivors and no stale claims |
| `2` | adjudication needed |
| `3` | the sweep was refused, or the scope was invalid |
| `1` | tool error |

### Convergence means *every* census mutant reached a conclusion

Exit `0` requires an empty survivor list, an empty stale list, **no timeouts,
and no unevaluated mutants**. A `TIMEOUT` is never conclusive and a census
entry the run did not reach is not a result, so either one keeps the run open.
The alternative is a sweep that exits `0` with 154 of 155 mutants untested.

Triage therefore reads the crate's **full census** (`just mutants-census`, a
`--list` with no build), never `mutants.out/mutants.json`. Both files in
`mutants.out/` are rewritten by every `just mutants` run, *including* the
narrowed kill-verification re-run this loop mandates — so triaging against them
asks "did everything the last run happened to select get killed?" and answers
yes. Measured: a one-mutant re-run left artefacts reading `1 mutants, 1 killed,
converged` for a 21-mutant crate; against the full census the same artefacts
read `21 mutants, 1 killed, 20 unevaluated`, exit 2.

### `--iterate` and the ledger are not the same thing

`--iterate` shrinks **compile time**: it skips mutants already caught in a
previous run. The ledger shrinks **judgment time**: it subtracts mutants
already adjudicated equivalent. Neither replaces the other.

> **`--iterate` is currently incompatible with the convergence contract.** Its
> whole point is to leave previously-caught mutants out of the artefacts, and
> this triage counts a census mutant with no verdict as unevaluated — so an
> `--iterate`d re-sweep can never report convergence, and one whose remaining
> mutants all survive is refused outright as `NothingKilled`. Reconciling the
> two needs the previous run's verdicts carried forward. Tracked in
> `pipe-cli-mutants-iterate-convergence-rjw`. Until then: do not pass `--iterate`.

---

## 2. Identity: why the key looks like that

```
key := <path> "::" <item> " @" <site> " " <class> ":" <from> "->" <to> " #" <occ>
```

Worked, from a live census:

```
crates/pipeline/pipeline-domain/src/projection.rs::current_phase @4ac941eb matchguard:s == stage->true #0
```

**No line numbers, anywhere.** A key that carried one would be invalidated by
every unrelated edit above it in the file, and a ledger whose entries expire
on unrelated edits is a ledger nobody trusts. `docs/0002-mutant-identity-by-holed-site-anchor.md`
has the measurement that settled it.

Every component is re-derived from the **current source text** by our own
`syn`-based normalizer:

| component | what it is |
|---|---|
| `path` | workspace-root-relative, forward slashes |
| `item` | the `::`-joined chain of enclosing `mod` / `impl` / `trait` / `fn`, generics erased, a trait impl spelled `Type#Trait` |
| `site` | `sha256(anchor_text)[..8]`, where the mutated span is replaced by the hole marker `‹›`, comments dropped, whitespace runs collapsed |
| `class` | one of `binop`, `unop`, `fnvalue`, `matcharm`, `matchguard` — a closed set |
| `from` | the normalized tokens **read from the source at the span** |
| `to` | the normalized replacement; `∅` when the replacement is empty |
| `occ` | 0-based rank inside a group that is otherwise identical, ordered by span start, computed by our own sort |

`from` is load-bearing, not decorative. A replacement alone does not identify a
mutation: `(report.rs, Verification::from_votes, BinaryOperator, "<")` matched
two distinct mutants in one measured crate.

### The anchor table

The site anchor is chosen by a **closed table on span kind**, not a generic
"smallest enclosing node" rule:

| mutated span | anchor |
|---|---|
| is, or is inside, a match-arm **guard** | arm pattern + `if` + holed guard. **The arm body is excluded** — editing an arm body must not invalidate a guard claim |
| **is** a whole match arm | the arm's pattern tokens, un-holed |
| **is** a whole function body | the function signature including its return type |
| is inside an `if` / `while` / `match` condition or scrutinee | the holed condition |
| is inside a `const` / `static` initializer | the holed declaration |
| otherwise | the smallest enclosing statement, holed |

One documented exception to "`from` is the tokens at the span": for a
`fnvalue` mutation the span is the *entire function body*. Inlining a body into
a key would make it unreadable and would invalidate it on any body edit, even
though the anchor is deliberately the signature. So `from` carries the
source-derived **return type** instead — `fnvalue:Option<ExecutionPhase>->None`.
Uniqueness still comes from `to`, which is distinct per replacement.

### Three scopes of invalidation

| edit | effect |
|---|---|
| inside the anchor | `site` changes → the mutant comes back as **NEW**. Correct: the claim was about that anchor |
| elsewhere in the same item | the key is intact but `item_digest` mismatches → **`StaleClaim`: NOT subtracted**, shown with the old justification and refutations attached |
| anywhere else | invisible to the key |

Re-affirming a stale claim requires a **fresh refutation record at a new
`tree_sha`**. Bumping the digest is not evidence, and `just mutants-reaffirm`
refuses to accept a record whose sha is already on file.

---

## 3. The ledger

`mutants-equivalence.toml` at the repo root, sibling to
`hex-lint-exceptions.toml` and for the same reason: a committed, reviewable
file of deliberate exceptions whose header states the rules a reviewer holds
the diff against.

**The ledger holds judgements, never computations.** Anything the pre-filter
re-derives each run — a vacuous mutation, an unviable one, one outside the
sweep's scope — is never written to it.

### A worked entry

```toml
[[equivalent]]
key         = "crates/pipeline/pipeline-domain/src/projection.rs::current_phase @4ac941eb matchguard:s == stage->true #0"
path        = "crates/pipeline/pipeline-domain/src/projection.rs"
item        = "current_phase"
site        = "4ac941eb"
site_text   = "WorkflowEvent::StageClaimed { stage: s, .. } if ‹›"
class       = "matchguard"
from        = "s == stage"
to          = "true"
occurrence  = 0
item_digest = "7c1d90ea4b62"          # WITNESS, not part of the key

snippet = """
-            WorkflowEvent::StageClaimed { stage: s, .. } if s == stage => {
+            WorkflowEvent::StageClaimed { stage: s, .. } if true => {
"""

argument      = "guarded-unreachable"
justification = "Every WorkflowEvent reaching this arm is already filtered to `stage` by the caller's per-stage partition, so `s == stage` is invariantly true here."

adjudicated_at = "ff4e1d0"
claimed_by     = "claude-opus-5/classify-1"

[[equivalent.refutation]]
lens           = "test-writer"
by             = "claude-opus-5/refute-1"
refuted        = false
method         = "test-written"
reason         = "constructed event logs mixing two StageIds; the arm is never reached with a foreign stage because the caller partitions first"
tree_sha       = "ff4e1d0"
baseline       = "green"
command        = "just mutants -p pipeline-domain -f crates/pipeline/pipeline-domain/src/projection.rs -F 'replace match guard s == stage with true in current_phase' --baseline skip"
verdict        = "survived"
test_committed = false
test_source    = '''
#[test]
fn current_phase_ignores_foreign_stage_claims() {
    let events: Vec<WorkflowEvent> = vec![claimed("other")];
    assert_eq!(current_phase(&events, &StageId::new("mine")), None);
}
'''

[[equivalent.refutation]]
lens               = "semantics"
by                 = "claude-opus-5/refute-2"
refuted            = false
method             = "impossible-by-construction"
impossible_because = "unreachable-by-type"
reason             = "the only construction path for this arm goes through the partitioned iterator; no public entry point admits a foreign StageId"
tree_sha           = "ff4e1d0"
```

### Closed enums

- `argument` ∈ `type-equivalent` · `guarded-unreachable` · `redundant-guard` ·
  `idempotent` · `unobservable-through-public-api` · `identical-tokens`
- `method` ∈ `test-written` · `impossible-by-construction`
- `impossible_because` ∈ `type-uninhabited` · `no-public-path` ·
  `identical-tokens` · `unreachable-by-type`
- `verdict` ∈ `killed` · `survived` · `timeout` · `unviable` — the **domain's**
  vocabulary, not any tool's console word. The first version compared a free
  string against one generator's spelling of "survived", hardcoded in the
  functional core, which made a refutation performed with any other runner
  permanently inadmissible
- `baseline` ∈ `green` · `red`
- `class` ∈ `binop` · `unop` · `fnvalue` · `matcharm` · `matchguard`
- `status` ∈ `active` · `revoked`

Each `argument` has a **standard of proof** the check enforces:

| argument | must bring |
|---|---|
| `type-equivalent`, `idempotent`, `unobservable-through-public-api` | a refutation with `method = "test-written"` — somebody tried to write the killing test |
| `guarded-unreachable`, `redundant-guard` | a refutation with `method = "impossible-by-construction"` — somebody showed the state unconstructible |
| `identical-tokens` | nothing: it is mechanically decidable, the pre-filter re-derives it every run, and it is **never admissible as a written entry** |

### What `ledger check` enforces

No sweep, no build. Blocking inside `just verify`.

1. `key == render(path, item, site, class, from, to, occurrence)`.
2. `justification` present, exactly one line, 60–400 characters after trim.
3. **Relevance** — the justification shares at least one identifier with the
   stored `snippet`.
4. **Banned boilerplate** (case-folded, punctuation-stripped): *equivalent*,
   *no behavioural change*, *trivial*, *obviously equivalent*,
   *same semantics*, *see above*.
5. **Anti-copy-paste** — a byte-identical justification across two different
   `item`s is an error. Allowed within one item.
6. `argument` in the enum, and it brings its standard of proof.
7. **Exactly two refutation records**, lenses `test-writer` and `semantics`,
   both `refuted = false`, with `by` values distinct from each other and from
   `claimed_by`.
8. `method = "test-written"` ⇒ `baseline == "green"`, `verdict == "survived"`,
   `test_source` containing `#[test]` or `proptest!`, and
   `test_committed == false`. Each of the other three verdicts is rejected with
   its own reason: `killed` **proves the claim false**, `timeout` is never
   conclusive, `unviable` is not evidence about tests.
9. `method = "impossible-by-construction"` ⇒ `impossible_because` in the enum.
   Prose alone is a schema error.
10. Every refutation's `tree_sha` is at least 7 hex characters **and resolves
    through `git cat-file`**. Unresolved it is a string, and re-affirmation —
    which refuses to repeat a sha already on record — degenerates into "supply
    a different string".
11. `status = "revoked"` ⇒ `revocation.by` resolves through `br show`, and
    `revocation.why` is non-empty.
12. No duplicate keys, **no unknown fields** (a typo must not silently disable
    a check), entries sorted by key, `schema == 1`.
13. The ledger **file exists**. An absent ledger is not an empty one: a bad
    rebase or a stray `git rm` would otherwise make the gate that protects the
    file report `0 entries OK` while every adjudication vanished.

Admission adds one rule the file cannot carry: **only a mutant that actually
SURVIVED may be claimed equivalent**, so `ledger admit` takes `--from` and
refuses a `timeout`, an `unviable` or a mutant with no verdict at all.

`just mutants-gates --census <list.json>` additionally reports dangling
entries — keys no mutant in that census matches. That is a sweep-loop check,
not part of `verify`.

### Revocation

A wrong entry is **revoked in place**, never deleted:

```toml
status = "revoked"

[equivalent.revocation]
by  = "pipe-cli-mutants-jq3.2.7"     # must be an existing bead id
at  = "a19f33c"
why = "br-xyz.3 added a direct entry point that bypasses the per-stage partition; the arm is now reachable with a foreign stage"
```

The tombstone is shown at triage so nobody re-adjudicates the same lie. Use
`just mutants-revoke '<key>' <bead> '<why>'`.

---

## 4. The classification convention

**Every remaining survivor is explicitly adjudicated. No survivor is left
unclassified.** There are two dispositions and no third:

- **equivalent** → a ledger entry with a one-line justification, admitted only
  through the two-refuter gate below;
- **test gap** → a bead describing the untested **behaviour** — *what*, not
  *how* — carrying the key and the kill command.

A survivor you have not reasoned about is a test gap. Uncertainty is never
evidence of equivalence: calling a test gap "equivalent" writes a permanent lie
into a committed ledger, while calling an equivalent a test gap only wastes
some time. The costs are not symmetric.

### Admission requires adversarial confirmation

An equivalence claim is admitted **only after two independent refuters have
each tried to refute it and failed**, through two different lenses
(`test-writer` and `semantics`). One agent's plausible verdict is not evidence.

The measurement behind this rule: arXiv:2501.12862 (ACH, Meta) scored an
LLM-based equivalent-mutant detector at **precision 0.79** raw — one in five
"equivalent" claims was wrong. Deterministic preprocessing before the model
judged lifted it to 0.95. That is why this loop pre-filters mechanically first
and then makes every surviving claim face independent refutation.

`just mutants-admit` refuses without two well-formed refutations from two
agents, neither of whom is the claimant. The gate runs *before* the write, so a
bad entry never reaches the file.

### NEVER BULLDOZE

> **Writing a contorted test to kill an equivalent mutant asserts the
> implementation, not the spec. Do not do it.**

This is a rule, not a preference. A test that exists only to move a mutation
counter pins whatever the code currently happens to do, and the next person to
change the implementation correctly will have to delete it — after spending an
afternoon working out whether it was protecting anything.

If you wrote such a test while trying to refute a claim, that is exactly what
was supposed to happen: record it in the ledger as
`refutation.test_source` with `test_committed = false`. It is **evidence**, not
a test. `ledger check` rejects `test_committed = true`.

### Test-gap beads

Describe the **behaviour** that is untested, not the fix. `just mutants-bead-draft <crate> '<key>'`
prints the key, the site anchor, the re-run, and the question the bead has to
answer.

---

## 5. Kill verification

```
kill-verification: just mutants -p <crate> -f <path> -F '<regex>' --baseline skip
  FLAG TRAP 1: -f/--file FILTERS files, it does not SCOPE the run. Without
  -p the run resolves against Cargo.toml's default-members, so -f on a file
  outside them matches NOTHING -- and a run that matches nothing exits
  successfully having tested zero mutants, which reads as `everything killed`.
  FLAG TRAP 2: -F is an alias of --re, a mutant-NAME regex matched against
  --list names. -F is NOT the file flag.
  --baseline skip is valid ONLY immediately after a green `just test` on this tree.
  This bead closes ONLY when that re-run reports KILLED.
  "a test was added" is never evidence.
```

That block is printed at the foot of every `just mutants-triage` report and at
the head of every generated test-gap bead, from a single constant, so the doc
and the tool cannot drift apart.

**A test-gap bead closes only when re-running exactly that mutant reports
KILLED.** "A test was added" is never evidence: the test may not reach the
mutated line, may not assert on the differing value, or may be gated behind a
feature the run does not enable.

Verified against `cargo mutants --help` v27:

| flag | what it does |
|---|---|
| `-p`, `--package <PACKAGE>` | **scopes** the run to a package. Without it the run resolves against `default-members` |
| `-f`, `--file <FILE>` | glob **filtering** files inside whatever the run is already scoped to |
| `-F`, `--re <EXAMINE_RE>` | regex for **mutations** to examine, matched against the names `--list` shows |
| `-E`, `--exclude-re <EXCLUDE_RE>` | the inverse of `-F` |

`-f` is a filter, not a scope, and the difference is not academic. Measured
on this tree: `cargo mutants --list -f crates/sandbox-protocol/src/lib.rs`
selects **zero** mutants, because the run is still scoped to
`default-members = ["apps/pipe-cli"]` and that file is not in it. A run that
selects nothing exits *successfully* having tested nothing. The first
kill-verification command this loop ever generated did exactly that — which is
why the generated command now always carries `-p`, and why the scope guard
refuses a bare `-f`.

Three things make this mechanical instead of a warning in a document:

- **The command is generated.** `just mutants-kill <crate> '<key>'` builds it from the
  domain's `KillCommand`, so the escaping lives in one place. Real descriptions
  carry regex metacharacters — `replace += with -= in modal_class`,
  `delete match arm StageStatus::Running{..}`.
- **The regex omits `line:col`**, so a command pasted into a bead stays valid
  across unrelated edits. When the selected group has more than one member, the
  rule is "all members report KILLED".
- **Selection is asserted at render time.** `cargo mutants -F` matching nothing
  exits *successfully* having tested zero mutants — which reads as "everything
  killed", a false green sitting directly on the kill-verification rule.
  `just mutants-kill` refuses to print a command that selects nothing.

### The `--baseline skip` hazard

`--baseline skip` makes a single-mutant re-run cheap by skipping the unmutated
build and test. On a **red tree every mutant looks killed** — it is a
false-green factory.

The tools emit `--baseline skip` only when a green stamp exists at
`target/mutation-triage/green-<fingerprint>`, written by `just mutants-green`.
A sweep whose baseline was skipped is **refused** without that stamp.

Two details of the stamp are load-bearing, and both were wrong at first:

- It is keyed to a fingerprint of the **working tree** — the commit plus every
  uncommitted change plus the contents of untracked files
  (`scripts/hooks/tree-fingerprint.sh`) — not to `git rev-parse HEAD`. The sha
  does not move when you edit a file, and editing tests at one commit is this
  loop's entire activity, so a sha-keyed stamp survives every *failed* attempt
  at a fix and then authorises a baseline skip on a red tree.
- It is written by `just test-all`, not `just test`. `just mutants` pins
  `--all-features`, and a bare `--workspace --all-targets` silently drops every
  target whose `required-features` fall outside the default resolution — so a
  stamp from the weaker suite would authorise skipping the baseline of the
  stronger one.

### Timeouts

**A `TIMEOUT` verdict is never admissible.** On this machine it is a
measurement artifact, not a result. Timeouts are quarantined by the pre-filter
into a separate section of the report, never folded into KILLED or SURVIVED,
and never adjudicated. Re-run the mutant with a raised `--timeout` before
classifying it either way.

Three mechanisms enforce it, because the rule was written in three places and
enforced in none: a timeout keeps the run's exit code at `2`, `ledger admit`
refuses a mutant whose verdict is not `survived`, and `ledger check` rejects a
refutation carrying `verdict = "timeout"`.

### Unviable mutants get their own list

A mutant whose mutated tree did not compile is not evidence about tests, but it
*is* evidence about the operator that produced it. Unviables are listed by key
alongside timeouts rather than folded into a summary count — a count gives
nobody a file, an item or an operator to look at.

---

## 6. Scope validity — the refusals

The most valuable thing this loop does is throw sweeps away.

### Stage 0 — the whole run is rejected, exit 3, no partial output

| condition | why |
|---|---|
| the unmutated baseline did not pass | red tree; every verdict is worthless |
| the mutated package owns **zero** test targets and the tested set was not widened | every mutant "survives" a suite that never ran |
| mutants ran and **nothing** was killed | the suite is not exercising this code |
| the census disagrees with the source at a span | the census is stale relative to the tree being triaged |
| the baseline was **skipped** and no green stamp covers this working tree | a skipped baseline on a red tree reports every mutant caught |
| the sweep contains **no mutants at all** | a run that selects nothing exits successfully having tested nothing |

### The sibling-test trap

There is no `.cargo/mutants.toml` in this repo, so `--test-workspace` defaults
to **false**: `just mutants -p <crate>` runs only *that crate's* tests. A domain
crate whose tests live in a sibling use-case crate therefore reports 100%
survivors — a fabricated test-gap avalanche of exactly the shape this loop
exists to eliminate.

`crates/classify-verify-domain` is the live example: 34 mutants, zero
`#[cfg(test)]` files, no `tests/` directory. Its tests live in
`classify-verify` and `classify-verify-agents`. A naive sweep reports 34 of 34
survivors.

```
just mutants-preflight classify-verify-domain     # exits 3 in seconds
```

prints the remedy:

```
just mutants -p classify-verify-domain \
    --test-package classify-verify --test-package classify-verify-agents
```

`classify-verify-domain` is the **regression fixture for this rule, never an
acceptance target**.

### Scope on the census itself

`Cargo.toml` sets `default-members = ["apps/pipe-cli"]`, so an unscoped
`cargo mutants --list` enumerates one app: **19 mutants versus 5295 with
`--workspace`** (both measured 2026-08-03). A ledger key validated against the
19 would be validated against 0.4% of the code, and "no dangling entries" would
mean nothing. `just mutants` and `just mutants-list` therefore **refuse** to run
without an explicit scope (`-p`, `-f`, `-D`, or `--workspace`).

Per the epic's standing rule, never sweep the whole workspace in one run: go
per-crate, per-file, or `--in-diff`.

---

## 7. The deterministic pre-filter

Runs before any agent judgment. Every rule fires with a reported id.

| rule | disposition |
|---|---|
| `verdict/unviable` | quarantine — it never compiled, so it is not evidence about tests |
| `verdict/timeout` | quarantine, re-run queue. Never adjudicated, never admissible |
| `scope/out-of-scope` | invisible to this run |
| `mutation/identical-tokens` | auto-equivalent — normalized `from` == normalized `to`. Re-derived each run, **never written to the ledger** |
| `mutation/default-form` | a duplicate *spelling* of a sibling mutant at the same site, collapsed |
| `ledger/hit` | subtracted — key and item digest both match |
| `ledger/stale` | **not subtracted** — shown with the old justification; needs a fresh refutation |
| `ledger/tombstone` | shown, with why the old verdict was wrong |
| `ledger/dormant` | a ledger entry this sweep did not cover — dormant, not orphaned |
| `sweep/duplicate-key` | collapsed, count surfaced |

Note on `mutation/default-form`: it collapses a duplicate **spelling**, it does
not adjudicate a mutant equivalent to the original. Replacing a body with
`vec![]` is emphatically not equivalent to the body; what *is* mechanical is
that a generator emitting both `vec![]` and `Default::default()` for the same
`-> Vec<T>` emitted one judgment twice. The surviving sibling still faces
judgment.

**Honest accounting.** Almost none of ACH's 0.79 → 0.95 preprocessing lift is
available here as *mechanical equivalence proof*: real mechanically-decidable
equivalents in Rust are rare, and small tightly-written domain crates produce
almost none. The lift here comes from **refusal** (throwing out invalid
sweeps), **quarantine** (timeouts and unviables never reach judgment) and
**clustering**. If this pre-filter is judged on "how many equivalents did it
auto-adjudicate", the honest answer is close to zero, and that is not a failure.

---

## 8. `--ignore-ledger` is a periodic obligation

```
cargo run -p mutants-triage-cli -- triage -p <crate> --from mutants.out/outcomes.json --ignore-ledger
```

Re-derives the survivor set from scratch and shows what the ledger is currently
hiding. It is not a debug flag: it is the only answer to a claim invalidated
from a distance — a change somewhere else in the tree that made a
`guarded-unreachable` argument false without touching the anchor or the item.

Run it whenever the crate's callers change shape, and at least whenever you
revisit the crate.

---

## 9. Generator neutrality

The report and ledger schemas are **generator-agnostic**. The planned operator
pack emits into this same pipeline.

> **Law:** a field belongs in the neutral schema only if it can be answered by
> reading the Rust source and the mutant's own diff. If answering it requires
> knowing which tool ran, it is provenance, not schema.

A generator supplies exactly five required things — `path`, `span`, `class`,
`to`, `verdict`. Everything else is optional corroboration or is derived by the
normalizer. Tool-specific payload rides opaquely in `Provenance`, which is
excluded from identity and from the ledger entirely.

Because identity is computed by the domain, **two generators mutating the same
site to the same value produce the same key** — a claim adjudicated under one
generator is honoured under the other, and vice versa.

Two gates hold this up:

- `just hex-lint` makes the *dependency* leak impossible: a domain or usecase
  crate cannot depend on a generator adapter.
- `just mutant-schema-neutral` closes the *vocabulary* leak: it greps the
  neutral core for one tool's name, its output paths, its class field, its
  result words and its two scope flags, and fails on any hit.

The kill-verification **rule** lives in the domain and is the same for every
generator; each adapter contributes only its own **flag traps** through
`KillCommandRenderer::flag_traps`, so a report or a drafted bead can never
instruct its reader to run a different tool's command. Every subcommand that
reads artefacts takes `--generator`, and the renderer is selected per
generator rather than bound once — a renderer pinned to one tool would keep
rendering that tool's flags for every other one, and the mismatch would compile
clean.

Adding the operator pack needs: a new driven-adapter crate with one
`impl SweepSource` and one `impl KillCommandRenderer`, one variant on the CLI's
generator enum (which the compiler then demands in `sweep_source` *and*
`kill_renderer`), and pack fixtures in the domain's conformance corpus. No
change to the ledger file, the filter, the classification convention, or any
existing entry.

> **Known gap.** `MutationClass` is still a closed five-variant enum whose
> variants are one generator's `genre` taxonomy, and `ledger check` parses the
> `class` component through it. A pack emitting an operator outside those five
> — a `saturating_*` ↔ `wrapping_*` swap, say — makes its own adapter refuse
> its own output. Opening the vocabulary means a new anchor-table fallback for
> classes nobody has emitted yet, so it belongs with the generator that needs
> it: `pipe-cli-mutants-open-class-vocabulary-4no`.

---

## 10. Where the code lives

| crate | role | what it owns |
|---|---|---|
| `mutation-triage-domain` | `domain` | identity, the ledger entities and their integrity rules, the pre-filter, the ports. No I/O, no generator vocabulary |
| `mutation-triage` | `usecase` | Stage 0 refusal, the sweep-to-report pipeline, ledger admission / re-affirmation / revocation |
| `cargo-mutants-report` | `driven-adapter` | the only crate that may name cargo-mutants |
| `mutation-triage-fs` | `driven-adapter` | the ledger's TOML format, source reading, the test-target inventory, bead resolution |
| `mutants-triage-cli` | `composition-root` | the `mutants-triage` binary and the wiring |

See also `docs/0002-mutant-identity-by-holed-site-anchor.md`.

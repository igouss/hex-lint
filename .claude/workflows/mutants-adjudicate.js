export const meta = {
  name: 'mutants-adjudicate',
  description: 'Adjudicate one crate\'s cargo-mutants survivors: pre-filter, classify, adversarially refute every equivalence claim, then ledger + file test-gap beads',
  whenToUse:
    'Run after EACH serial "just mutants -p <crate>" sweep. Args: {crate: "<crate name>", maxSurvivors: <int, default 60>}. Serves bead .1 (proof on real data), .2 (per-crate triage) and .10 (dogfooding the pack). Consumes mutants.out/outcomes.json -- it never launches a sweep itself.',
  phases: [
    { title: 'Census' },
    { title: 'Classify' },
    { title: 'Refute' },
    { title: 'Adjudicate' },
  ],
}

// `args` reaches a workflow as an object from some callers and as a JSON STRING
// from others. Reading `.crate` off a string yields undefined, so the guard below
// fired and the run died in 40ms with zero agents -- an invocation that looks
// correct at every call site. Normalise once, here; an unparseable string becomes
// no args at all, which the guard then reports honestly.
const ARGS = (function normaliseArgs(raw) {
  if (typeof raw !== 'string') {
    return raw || {}
  }
  try {
    return JSON.parse(raw)
  } catch (err) {
    return {}
  }
})(args)

// --- per-project inputs -----------------------------------------------------
// Subagents inherit none of the caller's context, so everything they need about
// THIS repo has to arrive here and be interpolated into every prompt.
const REPO = ARGS.repo || ''
const CRATE = ARGS.crate || ''
const MAX_SURVIVORS = ARGS.maxSurvivors || 60
// How this project files an issue. Overriding this is the ONLY change most
// projects need: the loop itself does not care what the tracker is.
const TRACKER =
  ARGS.tracker ||
  'Use the project\'s own issue tracker. If the repo has no tracker, write the issues into a\n' +
  'markdown file under docs/ instead and say so in the report.'
// Optional: an existing issue every test-gap issue should hang under.
const PARENT_ISSUE = ARGS.parentIssue || ''
const BATCHES = 4
const REFUTERS_PER_BATCH = 2

if (!REPO) {
  throw new Error('mutants-adjudicate requires args: {repo: "<absolute path to the repo>"} -- subagents get no cwd from the caller')
}
if (!CRATE) {
  throw new Error('mutants-adjudicate requires args: {crate: "<crate name>"} -- the crate whose sweep produced mutants.out/outcomes.json')
}

// EDIT POINT. This block is pasted into every subagent prompt verbatim. Replace
// it with your project's rules, or pass args:{law: "..."} to override without
// editing the file. The default below states only what this loop itself needs.
const LAW =
  ARGS.law ||
  `
REPO LAW (${REPO}). Violating any of these fails the work.
- Gates: run the project's own verify/check command before declaring done. NEVER bypass a
  pre-commit hook (no --no-verify).
- Tests: keep each test's cyclomatic complexity at 1 (no loops, no branches inside a test);
  cover zero / one / many; property tests prove the rules; keep integration tests few.
- Explicit type annotations on variables and lambda parameters. No 'unsafe'.
- Use 'cargo nextest run', not 'cargo test'. Targets behind required-features vanish from a
  plain nextest run AND from its skip count -- run the project's full-suite command when
  gated targets matter.
- Cargo is compile-bound and slow. Give every cargo Bash call a long timeout, and never run
  two cargo commands concurrently. NEVER 'cargo clean' (full rebuild).
- Commit style: follow the project's existing convention -- read 'git log' before writing one.
- Commit to the current branch; do NOT create a branch.
${TRACKER}
`

const FALSE_GREEN = `
FALSE-GREEN DISCIPLINE -- the theme of this entire epic.
- A convenient result is the trigger to VERIFY, not to move on.
- "a test was added" is NEVER evidence a mutant is dead. Only a re-run reporting KILLED for that
  exact mutant is.
- A TIMEOUT verdict on this machine is suspicious, not conclusive -- never fold it into KILLED or
  SURVIVED.
- A sweep with zero runnable tests makes every mutant "survive" a suite that never ran.
- NEVER fabricate a "verified". An honest "could not verify" is fine.
`

const CARGO_MUTANTS_FACTS = `
CARGO-MUTANTS v27 FACTS (verified; trust these over memory).
- Entry points are justfile recipes: 'just mutants -p <crate>', 'just mutants-list -p <crate>',
  'just mutants-diff'. They pin --all-features, --no-shuffle, PROPTEST_CASES=64 and
  TMPDIR=target/mutants-scratch. Every one of those flags is a trap-guard, not a preference.
- FLAG TRAP: '-f'/'--file' scopes files. '-F' is an alias of '--re' (mutant-NAME regex). '-F' is NOT
  the file flag. A kill-verification re-run of one mutant looks like:
      just mutants -f <path/to/file.rs> -F '<mutant name regex>'
- Parse mutants.out/outcomes.json. NEVER parse console output. mutants.out.old/ is the previous run.
- '--iterate' skips mutants already caught in previous runs; use it on every re-sweep.
- '--baseline skip' makes a single-mutant re-run cheap, but ONLY immediately after a green test run
  on the same tree. On a red tree every mutant looks killed.
`

const WHY_EQUIVALENCE_IS_DANGEROUS = `
WHY AN EQUIVALENCE VERDICT IS THE DANGEROUS ONE.
A mutant that survives is one of two things: an EQUIVALENT MUTANT (semantically identical to the
original -- unkillable, genuine noise) or a TEST GAP (real, and the whole point of the exercise).

Calling a test gap "equivalent" writes a permanent lie into a committed ledger and hides that gap
forever. Calling an equivalent "a test gap" only wastes some time. The costs are NOT symmetric.

arXiv:2501.12862 (ACH, Meta) measured LLM equivalent-mutant detection at precision 0.79 raw -- one
in five "equivalent" claims was wrong. Deterministic preprocessing before the model judged lifted it
to 0.95. That is exactly why this workflow pre-filters mechanically first and then makes every
surviving equivalence claim face independent refutation.

THEREFORE: when you are uncertain, the answer is TEST GAP. Uncertainty is never evidence of
equivalence.
`

// ---------------------------------------------------------------------------

const CENSUS_SCHEMA = {
  type: 'object',
  required: ['sweepValid', 'invalidReason', 'crate', 'counts', 'survivors', 'timeouts', 'autoAdjudicated'],
  properties: {
    sweepValid: {
      type: 'boolean',
      description: 'false if the sweep cannot be trusted at all: outcomes.json missing/empty, zero tests ran, baseline failed, or the run was cut short',
    },
    invalidReason: { type: 'string', description: 'empty string when sweepValid is true' },
    crate: { type: 'string' },
    counts: {
      type: 'object',
      required: ['total', 'killed', 'survived', 'timeout', 'unviable', 'ledgerFiltered'],
      properties: {
        total: { type: 'integer' },
        killed: { type: 'integer' },
        survived: { type: 'integer' },
        timeout: { type: 'integer' },
        unviable: { type: 'integer' },
        ledgerFiltered: { type: 'integer', description: 'survivors dropped because the ledger already adjudicates them equivalent' },
      },
    },
    survivors: {
      type: 'array',
      description: 'NEW survivors only -- ledger entries already subtracted. Never includes timeouts.',
      items: {
        type: 'object',
        required: ['key', 'file', 'item', 'mutation', 'occurrence', 'sourceContext'],
        properties: {
          key: { type: 'string', description: 'stable ledger identity key for this mutant' },
          file: { type: 'string' },
          item: { type: 'string', description: 'enclosing function or item path, NOT a line number' },
          mutation: { type: 'string', description: 'what the mutation does, e.g. replace < with <=' },
          occurrence: { type: 'integer' },
          sourceContext: { type: 'string', description: 'the mutated line plus enough surrounding source to judge it' },
        },
      },
    },
    timeouts: {
      type: 'array',
      description: 'TIMEOUT verdicts, kept separate because on this machine they are suspicious, not conclusive',
      items: {
        type: 'object',
        required: ['key', 'file', 'item', 'mutation'],
        properties: {
          key: { type: 'string' },
          file: { type: 'string' },
          item: { type: 'string' },
          mutation: { type: 'string' },
        },
      },
    },
    autoAdjudicated: {
      type: 'array',
      description: 'survivors mechanically resolved by deterministic pre-filter, with the rule that resolved each. These skip agent judgment entirely.',
      items: {
        type: 'object',
        required: ['key', 'rule', 'verdict'],
        properties: {
          key: { type: 'string' },
          rule: { type: 'string' },
          verdict: { type: 'string', enum: ['equivalent', 'test-gap'] },
        },
      },
    },
  },
}

const CLASSIFY_SCHEMA = {
  type: 'object',
  required: ['verdicts'],
  properties: {
    verdicts: {
      type: 'array',
      items: {
        type: 'object',
        required: ['key', 'verdict', 'concern', 'reasoning', 'untestedBehaviour', 'generatorGap'],
        properties: {
          key: { type: 'string' },
          verdict: {
            type: 'string',
            enum: ['equivalent', 'test-gap'],
            description: 'when uncertain the answer is test-gap; uncertainty is never evidence of equivalence',
          },
          concern: {
            type: 'string',
            description: 'the CONCERN this mutant represents, as a short reusable label, e.g. "framing arithmetic", "protocol state transition", "boundary safety", "error propagation". This label is what later turns a pile of survivors into an operator candidate.',
          },
          reasoning: { type: 'string', description: 'why, referencing the actual source' },
          untestedBehaviour: {
            type: 'string',
            description: 'for test-gap only: the BEHAVIOUR that is unasserted, stated as what not how, ready to become a bead. Empty for equivalent.',
          },
          generatorGap: {
            type: 'boolean',
            description: 'true if while reading this site you can name a plausible bug shape for THIS codebase that cargo-mutants generic operators cannot even generate. Category (c) candidate for the operator pack.',
          },
        },
      },
    },
  },
}

const REFUTE_SCHEMA = {
  type: 'object',
  required: ['lens', 'rulings'],
  properties: {
    lens: { type: 'string' },
    rulings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['key', 'refuted', 'reason'],
        properties: {
          key: { type: 'string' },
          refuted: {
            type: 'boolean',
            description: 'true means the equivalence claim is WRONG -- this mutant is killable, it is a test gap. Default to true when uncertain.',
          },
          reason: { type: 'string' },
          killingTestSketch: {
            type: 'string',
            description: 'when refuted: the concrete input or state that distinguishes mutant from original',
          },
        },
      },
    },
  },
}

// ---------------------------------------------------------------------------

function chunk(items, buckets) {
  const size = Math.max(1, Math.ceil(items.length / buckets))
  const out = []
  let i = 0
  while (i < items.length) {
    out.push(items.slice(i, i + size))
    i += size
  }
  return out
}

// ---------------------------------------------------------------------------

phase('Census')

const census = await agent(
  `Read the cargo-mutants sweep results for crate "${CRATE}" and produce a trustworthy survivor census.

Working directory: ${REPO}

Do exactly this, in order:

1. VALIDATE THE SWEEP BEFORE READING ANY VERDICT. A sweep you cannot trust must be reported as
   sweepValid=false, and you must NOT launder its numbers into a census. Check all of:
   - mutants.out/outcomes.json exists, is non-empty, and parses;
   - it is actually the sweep for "${CRATE}" (not a stale run for another crate -- check the mutant
     file paths against the crate's source directory, and compare mtimes);
   - the unmutated baseline PASSED. If the baseline failed or was skipped on an unverified tree,
     every mutant looks killed and the whole run is a false-green factory;
   - tests actually ran. A target that resolved to zero runnable tests makes every mutant "survive"
     a suite that never executed -- the same false-green shape as the gated-features trap.
   If any check fails, set sweepValid=false, explain precisely in invalidReason, and return empty
   arrays. Do not guess. Do not partially salvage.

2. Read the equivalence ledger and its documented format -- docs/MUTATION_TRIAGE.md defines it,
   and the ledger itself is mutants-equivalence.toml at the repo root. Build the identity key for
   each mutant exactly the way that format specifies. If no ledger exists yet, say so in
   invalidReason but continue with sweepValid=true and ledgerFiltered=0.

3. Subtract ledger entries from the survivor list. Report how many were subtracted.

4. Keep TIMEOUT verdicts in their own array. Never fold them into killed or survived: on this
   machine a timeout is suspicious, not conclusive.

5. DETERMINISTIC PRE-FILTER -- run this BEFORE any judgment, and report each auto-resolution with
   the rule that fired. This is the step that lifted ACH's precision from 0.79 to 0.95. Only
   MECHANICAL, uncontestable rules qualify. Examples of the shape:
   - a mutation inside code that cannot compile under the tested feature set;
   - a mutation whose spliced form is textually identical to the original;
   - a mutation in code that is provably unreachable from any test entry point by construction
     (not "I doubt it is tested" -- that is judgment, and judgment belongs in the next phase).
   If a rule requires you to reason about intent, it is NOT mechanical. Leave it for the classifier.

6. For every remaining survivor, capture sourceContext: the mutated line plus enough surrounding
   source that someone who has not opened the file can judge it. Include the function signature.

7. Cap the returned survivors array at ${MAX_SURVIVORS}. If you drop any, say so explicitly in
   invalidReason with the exact number dropped -- a silent cap reads as "we covered everything" when
   we did not.

${LAW}
${FALSE_GREEN}
${CARGO_MUTANTS_FACTS}`,
  { label: `census:${CRATE}`, schema: CENSUS_SCHEMA, agentType: 'general-purpose' },
)

if (!census || !census.sweepValid) {
  log(`SWEEP REJECTED for ${CRATE}: ${census ? census.invalidReason : 'census agent failed'}`)
  return {
    crate: CRATE,
    adjudicated: false,
    reason: census ? census.invalidReason : 'census agent failed',
    guidance: 'Nothing was ledgered and no beads were filed. Fix the sweep and re-run this workflow -- adjudicating an untrustworthy sweep is exactly the false green this epic exists to prevent.',
  }
}

log(
  `${CRATE}: ${census.counts.total} mutants, ${census.counts.survived} survived, ` +
    `${census.counts.timeout} timeout, ${census.counts.ledgerFiltered} already ledgered, ` +
    `${census.autoAdjudicated.length} auto-adjudicated, ${census.survivors.length} for judgment`,
)

if (census.survivors.length === 0) {
  log(`${CRATE}: nothing left to judge after pre-filtering`)
}

// ---------------------------------------------------------------------------
// Classify, then refute. Pipelined, not barriered: a batch's equivalence claims
// go to refutation the moment that batch is classified, while other batches are
// still being read.
// ---------------------------------------------------------------------------

const batches = chunk(census.survivors, BATCHES)

const REFUTE_LENSES = [
  {
    key: 'test-writer',
    brief: `You are a TEST WRITER. For each claim, try to CONSTRUCT a concrete test that distinguishes
    the mutant from the original: a specific input, a specific state, a specific observable output
    that differs. Reach through the crate's real public API. If you can name such an input, the
    equivalence claim is REFUTED. Only if you can demonstrate that NO input can distinguish them --
    and can say why, structurally -- is the claim left standing.`,
  },
  {
    key: 'semantics',
    brief: `You are a SEMANTICIST. For each claim, reason about observability rather than tests. Is
    the mutated expression's result ever observable through any public path -- return value, error,
    logged side effect, downstream state? Watch specifically for claims that lean on "this is
    unreachable" or "callers never pass that value": an unenforced precondition is a TEST GAP, not
    an equivalence -- the type system is the only thing that makes an input impossible. Also watch
    for claims that lean on a saturating/wrapping or overflow argument without checking the actual
    integer widths involved.`,
  },
]

const adjudicatedBatches = await pipeline(
  batches,
  (batch, _orig, index) =>
    agent(
      `Classify ${batch.length} surviving mutants from crate "${CRATE}" (batch ${index + 1} of ${batches.length}).

Working directory: ${REPO}

${WHY_EQUIVALENCE_IS_DANGEROUS}

THE SURVIVORS:
${JSON.stringify(batch, null, 2)}

For each one:
1. OPEN THE ACTUAL SOURCE FILE. The sourceContext is a pointer, not a substitute. Read the enclosing
   function, its callers, and its tests before ruling.
2. Rule 'equivalent' or 'test-gap'. When uncertain: test-gap. Every 'equivalent' ruling you make
   will be independently attacked in the next phase, so a lazy one costs the run time -- but a lazy
   'test-gap' only costs a bead that turns out to be easy. Bias accordingly.
3. Give it a CONCERN label -- a short, reusable name for the KIND of fault this mutant represents,
   not a description of this one site. Reuse labels across mutants that share a shape. This label is
   what later turns a pile of survivors into an operator candidate rather than a list. Good labels
   look like: "framing arithmetic", "boundary comparison", "protocol state transition", "error
   propagation dropped", "overflow discipline".
4. For test-gap: state the untested BEHAVIOUR -- what is unasserted, in spec terms. What, not how.
   It becomes a bead verbatim, and the implementing agent is smart enough to work out the how.
   Do NOT propose a test that asserts the implementation; that is bulldozing, and it is forbidden.
5. Set generatorGap=true if, reading this site, you can name a plausible bug shape for THIS codebase
   that cargo-mutants' generic operators cannot even generate. You are allowed -- encouraged -- to
   invent concern-specific mutants here; this is upstream evidence gathering, not the deterministic
   pipeline. Say what the mutant would be in the reasoning field.

Do NOT edit any file. Do NOT run cargo. Do NOT file beads. This phase reads and rules; a later
serialized phase does all the writing.

${LAW}
${FALSE_GREEN}
${CARGO_MUTANTS_FACTS}`,
      { label: `classify:${CRATE}:${index + 1}`, phase: 'Classify', schema: CLASSIFY_SCHEMA, agentType: 'general-purpose' },
    ),
  (classified, _orig, index) => {
    const verdicts = (classified && classified.verdicts) || []
    const claims = verdicts.filter((v) => v.verdict === 'equivalent')
    if (claims.length === 0) {
      return { verdicts, rulings: [], refutedKeys: [] }
    }
    return parallel(
      REFUTE_LENSES.map((lens) => () =>
        agent(
          `REFUTE these equivalence claims from crate "${CRATE}" (batch ${index + 1}). Your job is to prove
them WRONG. A pass that refutes nothing has not tried.

Working directory: ${REPO}

${WHY_EQUIVALENCE_IS_DANGEROUS}

An admitted equivalence entry goes into a COMMITTED ledger and permanently hides that mutant from
every future sweep. If the claim is wrong, a real test gap is buried forever and nobody will ever
look at it again. That is why you exist.

THE CLAIMS TO ATTACK:
${JSON.stringify(claims, null, 2)}

YOUR LENS:
${lens.brief}

DEFAULT TO refuted=true. You may only return refuted=false when you can state, structurally, why NO
observable difference exists. "I could not think of a distinguishing input" is refuted=true, not
refuted=false -- absence of your imagination is not evidence of equivalence.

Open the real source. Do NOT edit files. Do NOT run cargo. Report rulings only.

${LAW}
${FALSE_GREEN}`,
          { label: `refute:${CRATE}:${index + 1}:${lens.key}`, phase: 'Refute', schema: REFUTE_SCHEMA, agentType: 'general-purpose' },
        ),
      ),
    ).then((rulings) => {
      const live = rulings.filter(Boolean)
      // Conservative quorum: an equivalence claim is admitted ONLY if EVERY lens
      // failed to refute it. One refutation demotes it to a test gap. This is the
      // asymmetric-cost rule made mechanical -- a wrong "equivalent" is permanent,
      // a wrong "test gap" is merely a bead someone closes quickly.
      const refutedKeys = live
        .flatMap((r) => r.rulings || [])
        .filter((r) => r.refuted)
        .map((r) => r.key)
      const demoted = new Set(refutedKeys)
      const finalVerdicts = verdicts.map((v) =>
        v.verdict === 'equivalent' && demoted.has(v.key)
          ? { ...v, verdict: 'test-gap', reasoning: `${v.reasoning}\n[DEMOTED by refutation: equivalence claim was refuted]` }
          : v,
      )
      return { verdicts: finalVerdicts, rulings: live, refutedKeys: Array.from(demoted) }
    })
  },
)

const settled = adjudicatedBatches.filter(Boolean)
const allVerdicts = settled.flatMap((b) => b.verdicts || [])
const allRulings = settled.flatMap((b) => b.rulings || [])
const demotedCount = settled.flatMap((b) => b.refutedKeys || []).length

const equivalents = allVerdicts.filter((v) => v.verdict === 'equivalent')
const testGaps = allVerdicts.filter((v) => v.verdict === 'test-gap')
const generatorGaps = allVerdicts.filter((v) => v.generatorGap)

log(
  `${CRATE} adjudicated: ${equivalents.length} equivalent (survived refutation), ` +
    `${testGaps.length} test gaps (${demotedCount} demoted by refutation), ` +
    `${generatorGaps.length} operator-pack candidates`,
)

// ---------------------------------------------------------------------------
// One writer. Parallel agents appending to the same ledger file would race and
// silently drop entries -- so every mutation of repo state happens here, once.
// ---------------------------------------------------------------------------

phase('Adjudicate')

const written = await agent(
  `Write up the adjudication of crate "${CRATE}"'s mutation survivors. You are the ONLY writer in this
run -- every ledger entry, bead and report fragment comes from you, so nothing races.

Working directory: ${REPO}

SWEEP CENSUS:
${JSON.stringify(census.counts, null, 2)}

MECHANICALLY AUTO-ADJUDICATED (deterministic pre-filter, no judgment involved):
${JSON.stringify(census.autoAdjudicated, null, 2)}

TIMEOUT VERDICTS -- suspicious, NOT conclusive, and deliberately not classified:
${JSON.stringify(census.timeouts, null, 2)}

FINAL VERDICTS (equivalence claims already survived independent adversarial refutation;
${demotedCount} were demoted to test-gap because a refuter broke them):
${JSON.stringify(allVerdicts, null, 2)}

REFUTATION RULINGS, for the justification text:
${JSON.stringify(allRulings, null, 2)}

Do this:

1. LEDGER. For each verdict 'equivalent', add an entry in the format documented in
   docs/MUTATION_TRIAGE.md -- read it, do not invent a format. Each entry's justification must
   state why it is equivalent AND note that it survived refutation by ${REFUTERS_PER_BATCH}
   independent lenses. Do not add an entry without a justification.

2. TEST-GAP ISSUES. For each verdict 'test-gap', file an issue. Requirements:
   - the issue describes the untested BEHAVIOUR -- what, not how. The implementing agent is smart
     and works out the how; an issue that prescribes an implementation is a bad issue.
   - it records the mutant's identity key and the exact kill-verification command that closes it.
     Generate that command with 'just mutants-kill <crate> "<key>"' -- never type one by hand.
     FLAG TRAP, state it in the issue: '-f'/'--file' FILTERS files, it does not SCOPE the run;
     '-F' is an alias of '--re' (mutant-NAME regex) and is NOT the file flag. A command that
     selects nothing exits successfully having tested nothing, which reads as "everything killed".
   - it states the closing rule explicitly: this issue closes ONLY when that re-run reports KILLED.
     "a test was added" is not evidence.
   - CLUSTER BY BEHAVIOUR, not by mutant. Several survivors on one unvisited branch are ONE issue.
   - Before creating, check for an existing issue covering the same behaviour and update it
     instead of creating a duplicate.${PARENT_ISSUE ? `\n   - hang it under the existing issue ${PARENT_ISSUE}.` : ''}

3. TIMEOUTS. Do not classify them. Record them in the report fragment as "unresolved: re-run with a
   raised --timeout before classifying". If there are many, file ONE issue covering the re-run.
   A timeout can also be a provably NON-TERMINATING mutant (an index advanced by '*= 1' never
   leaves 0). Read the mutation before blaming the machine: raising the timeout will not change
   that verdict, and saying so in the issue saves the next agent a sweep.

4. REPORT FRAGMENT. Append a per-crate fragment to docs/mutants-survivor-evidence.md (create it if
   this is the first crate). It must carry: the sweep counts, the mutation score, survivors grouped
   BY CONCERN LABEL, the ledger entries added, the test-gap issues filed with their ids, the
   timeouts left unresolved, and -- separately and prominently -- every verdict where generatorGap
   is true, with its example site and the proposed mutant shape. Record proposed mutants even when
   they are not implementable today.

5. Commit. Follow the project's existing commit convention -- read 'git log' first. Let the
   pre-commit hooks run -- NEVER --no-verify. Commit to the current branch; do NOT create a branch.
   Use THIS session's own co-author and session trailers if the project uses them; do not copy a
   session id out of any file.

Report: ledger entries added, issue ids filed, timeouts left open, category (c) candidates found,
and the commit SHA.

${LAW}
${FALSE_GREEN}
${CARGO_MUTANTS_FACTS}`,
  { label: `adjudicate:${CRATE}`, agentType: 'general-purpose' },
)

return {
  crate: CRATE,
  adjudicated: true,
  counts: census.counts,
  ledgered: equivalents.length,
  testGaps: testGaps.length,
  demotedByRefutation: demotedCount,
  timeoutsUnresolved: census.timeouts.length,
  operatorPackCandidates: generatorGaps.map((v) => ({ key: v.key, concern: v.concern, reasoning: v.reasoning })),
  written,
}

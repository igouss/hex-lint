// EXAMPLE Claude Code workflow — the smallest useful thing you can build on
// mutation-triage. Read this before mutants-adjudicate.js: same shape, a
// tenth the size.
//
// What it does: reads one crate's triage output, classifies the survivors in
// parallel, and writes a markdown report. It writes ONE file and commits
// nothing, so it is safe to run before you trust the full adjudicator.
//
// What it deliberately does NOT do: launch a sweep, touch the ledger, or file
// issues. Those are mutants-adjudicate.js's job.
//
// Run:
//   just mutants -p my-crate          # serially, off-hours, NOT from here
//   Workflow({ scriptPath: "examples/mutants-triage-report.workflow.js",
//              args: { repo: "/abs/path/to/repo", crate: "my-crate" } })

export const meta = {
  name: 'mutants-triage-report',
  description: 'Read one crate\'s mutation survivors, classify them in parallel, write a markdown report',
  whenToUse:
    'After a serial `just mutants -p <crate>` sweep, when you want a readable survivor report without touching the ledger. Args: {repo, crate}. A worked minimal example of the mutation-triage workflow shape.',
  phases: [{ title: 'Read' }, { title: 'Classify' }, { title: 'Report' }],
}

// --- arguments --------------------------------------------------------------
// `args` arrives as an object from some callers and as a JSON string from
// others; reading a field off a string yields undefined and the guard below
// then fires on a call that looks correct. Normalise once.
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

const REPO = ARGS.repo || ''
const CRATE = ARGS.crate || ''
const BATCH_SIZE = ARGS.batchSize || 12

if (!REPO || !CRATE) {
  throw new Error('mutants-triage-report requires args: {repo: "<absolute repo path>", crate: "<crate name>"}')
}

// Subagents inherit NOTHING from the calling session — not the working
// directory, not the repo's rules, not the tool's traps. Everything they need
// has to be interpolated into their prompt. This block is why the workflow
// works; do not trim it.
const TOOL_FACTS = `
MUTATION-TRIAGE FACTS (trust these over memory).
- 'just mutants-triage <crate>' is the ONLY thing you read. Exit codes:
    0 converged | 2 adjudication needed | 3 sweep refused/invalid scope | 1 tool error.
- NEVER parse mutants.out/ yourself. Both files there are rewritten by every 'just mutants'
  run, including the narrowed single-mutant re-runs the loop mandates, so they answer
  "did everything the last run selected get killed?" -- and answer yes.
- NEVER run 'just mutants' (a sweep) from inside this workflow. Sweeps are compile-bound;
  agents running cargo concurrently contend for target/ and manufacture TIMEOUT verdicts
  that read as kills. Sweeps are driven serially by a human or a background shell.
- A TIMEOUT is never conclusive. Do not classify one as killed or survived. Note that a
  timeout can also be a provably NON-TERMINATING mutant (an index advanced by '*= 1' never
  leaves 0) -- read the mutation before blaming the machine.
- A surviving mutant is exactly one of two things: an EQUIVALENT mutant (semantically
  identical to the original, unkillable, noise) or a TEST GAP (real).
`

const READ_SCHEMA = {
  type: 'object',
  required: ['usable', 'reason', 'exitCode', 'counts', 'survivors'],
  properties: {
    usable: {
      type: 'boolean',
      description: 'false if triage exited 3, or exited 0 with nothing to report',
    },
    reason: { type: 'string', description: 'empty when usable is true' },
    exitCode: { type: 'integer' },
    counts: {
      type: 'object',
      required: ['total', 'killed', 'survived', 'timeout', 'unviable', 'ledgerFiltered'],
      properties: {
        total: { type: 'integer' },
        killed: { type: 'integer' },
        survived: { type: 'integer' },
        timeout: { type: 'integer' },
        unviable: { type: 'integer' },
        ledgerFiltered: { type: 'integer' },
      },
    },
    survivors: {
      type: 'array',
      description: 'NEW survivors only, exactly as triage printed them. Never includes timeouts.',
      items: {
        type: 'object',
        required: ['key', 'file', 'item', 'siteText'],
        properties: {
          key: { type: 'string', description: 'the stable identity key triage printed' },
          file: { type: 'string' },
          item: { type: 'string', description: 'enclosing function or item path' },
          siteText: { type: 'string', description: 'the holed source line triage printed' },
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
        required: ['key', 'likely', 'concern', 'reasoning'],
        properties: {
          key: { type: 'string' },
          likely: {
            type: 'string',
            enum: ['test-gap', 'equivalent', 'unsure'],
            description: 'A LIKELIHOOD, not a verdict. Admitting an equivalent needs two independent refuters; this workflow admits nothing.',
          },
          concern: {
            type: 'string',
            description: 'short label grouping this with its siblings, e.g. "boundary comparison", "operation elision"',
          },
          reasoning: {
            type: 'string',
            description: 'why. Name the concrete input that distinguishes mutant from original, or say why none exists.',
          },
        },
      },
    },
  },
}

// --- Read -------------------------------------------------------------------

phase('Read')

const sweep = await agent(
  `Read the mutation-triage report for crate "${CRATE}" and return it as structured data.

Working directory: ${REPO}

Do exactly this:
1. Run: just mutants-triage ${CRATE}
   Capture its FULL output to a file and read the file. Do NOT pipe it through 'head' or
   'tail' -- bounding output at capture time closes the pipe, which changes the exit code
   you are trying to read.
2. Record the real exit code. If it is 3, the sweep was refused: set usable=false and put
   the printed reason in 'reason'. Do not try to work around a refusal.
3. If it is 0, the crate has converged: set usable=false with reason "converged".
4. If it is 2, transcribe every listed survivor into the schema. Transcribe, do not
   summarise: the key must be byte-identical to what triage printed.

Do NOT run a sweep. Do NOT edit any file.

${TOOL_FACTS}`,
  { label: 'read', schema: READ_SCHEMA },
)

log(`triage exit ${sweep.exitCode}: ${sweep.counts.survived} survivors, ${sweep.counts.timeout} timeouts`)

// A barrier is justified here: with nothing to classify there is no phase 2 at
// all, and an early exit beats fanning out over an empty list.
if (!sweep.usable || sweep.survivors.length === 0) {
  log(`nothing to classify: ${sweep.reason || 'no survivors'}`)
  return { crate: CRATE, counts: sweep.counts, classified: 0, reason: sweep.reason }
}

// --- Classify ---------------------------------------------------------------

phase('Classify')

// Batch rather than one-agent-per-survivor: sibling mutants on the same branch
// are only recognisable as siblings when one agent sees them together. Three
// survivors on one branch is the signal that the branch is unvisited.
function intoBatches(items, size) {
  const out = []
  let i = 0
  while (i < items.length) {
    out.push(items.slice(i, i + size))
    i += size
  }
  return out
}

const batches = intoBatches(sweep.survivors, BATCH_SIZE)
log(`${sweep.survivors.length} survivors in ${batches.length} batch(es)`)

const classified = await parallel(
  batches.map((batch, index) => () =>
    agent(
      `Classify these ${batch.length} surviving mutants from crate "${CRATE}".

Working directory: ${REPO}

THE SURVIVORS:
${JSON.stringify(batch, null, 2)}

For each one: READ THE SOURCE around the site and read the crate's tests. Then decide
whether the mutant is most likely a TEST GAP or an EQUIVALENT mutant.

The question that settles it: is there ANY input for which the mutant and the original
behave differently? If yes it is a test gap, and your reasoning must name that input. If
you can show no such input exists, it is likely equivalent -- but say 'unsure' rather than
guess, because this workflow does not admit equivalents and a soft 'unsure' costs nothing.

Watch for the shape that matters: several survivors on the SAME branch mean that branch is
never entered by any test. Give them the same concern label so the report groups them.

Do NOT write any file. Do NOT run a sweep. Report only.

${TOOL_FACTS}`,
      { label: `classify:${index + 1}`, phase: 'Classify', schema: CLASSIFY_SCHEMA },
    ),
  ),
)

// parallel() resolves a failed thunk to null rather than rejecting, so filter
// before use. Merging in plain JS keeps the counts honest -- no agent in the merge.
const verdicts = classified.filter(Boolean).flatMap((r) => r.verdicts || [])
const gaps = verdicts.filter((v) => v.likely === 'test-gap')
const equivalents = verdicts.filter((v) => v.likely === 'equivalent')

log(`${verdicts.length} classified: ${gaps.length} test-gap, ${equivalents.length} likely-equivalent`)

// --- Report -----------------------------------------------------------------

phase('Report')

const report = await agent(
  `Write a survivor report for crate "${CRATE}" to docs/mutants-report-${CRATE}.md.

Working directory: ${REPO}

THE COUNTS:
${JSON.stringify(sweep.counts, null, 2)}
timeouts unresolved: ${sweep.counts.timeout}

THE CLASSIFICATIONS (${verdicts.length}):
${JSON.stringify(verdicts, null, 2)}

The report must carry, in this order:
1. The counts and the mutation score, stated as a fraction, not a percentage alone.
2. Survivors GROUPED BY CONCERN LABEL, not listed flat. A group of three on one branch is
   the report's most useful sentence -- say what that branch is and that no test enters it.
3. The likely-equivalent list, marked clearly as UNCONFIRMED. State that admitting any of
   them to the ledger requires two independent refuters, neither of them the classifier,
   and that this report performed no refutation.
4. The ${sweep.counts.timeout} timeout(s), listed and explicitly left unclassified.

Write the file. Commit nothing. Do not touch mutants-equivalence.toml.

Report back the path you wrote and the mutation score.

${TOOL_FACTS}`,
  { label: 'report' },
)

return {
  crate: CRATE,
  counts: sweep.counts,
  classified: verdicts.length,
  likelyTestGaps: gaps.length,
  likelyEquivalents: equivalents.length,
  timeoutsUnresolved: sweep.counts.timeout,
  report,
  nextStep:
    equivalents.length > 0
      ? `Nothing was admitted to the ledger — this workflow cannot. Run mutants-adjudicate.js on "${CRATE}" to attack the ${equivalents.length} likely-equivalent claim(s) with two independent refuters.`
      : `No equivalence candidates. File the ${gaps.length} test gap(s) as issues, clustered by concern, then close each one only when 'just mutants-kill ${CRATE} "<key>"' reports KILLED.`,
}

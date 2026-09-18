# bug-replay — the "before production" benchmark

Main-branch snapshots only show the bugs that survived to main: dead paths.
What a type checker is actually for is catching the regression at PR time,
before production. There is no Ruby equivalent of a real-bug replay
benchmark; the scripts in this directory build the first one and judge `ita`
on it two-sidedly, head-to-head with Sorbet.

## The pipeline

```
git history (public repo clone)
        │  mine.rb      — walk `git log -p`, emit bug/fix PAIRS (JSONL)
        ▼
candidates/<id>.jsonl          one record per type-shaped bug fix:
        │                      {id, fix_sha, parent_sha, subject, files,
        │                       hunks[{path, old_start, new_start,
        │                              removed[], added[]}], signal,
        │                              also:[dropped fix_sha]}
        │  replay.sh    — worktree both revisions, check both, judge
        ▼
results/<id>.jsonl             one line per record judged:
                               record + {"ita": {...}, "srb": {...}}
```

The `candidates/*.jsonl` records embed short verbatim diff hunks from
the mined repos — [`candidates/NOTICE.md`](candidates/NOTICE.md) states
each repo's license and the per-record sha+path attribution.

`judge.rb` classifies each pair against one tool's diagnostics at both
revisions:

| label | meaning |
|---|---|
| HIT | a diagnostic on a touched file at the buggy parent is GONE at the fix — the checker saw the regression and the fix silenced it |
| MISS | silent on both sides — a false negative. Acceptable under Invariant #1 (silence is always acceptable), but it is exactly the recall this benchmark measures |
| NOISE | a diagnostic at the FIX the parent did not have — a potential false positive on freshly fixed code. Printed loudly (`*** NOISE`) and this is the headline of any report: a wrong diagnostic is the one thing this project must never do |
| UNRELATED | diagnostics present at the parent, unchanged at the fix — the tool fires, but not on this bug |
| HIT? | a HIT whose silenced diagnostic's family does not match the commit subject's failure vocabulary (`class_match=false`, e.g. subject says ArgumentError but the silenced diagnostic is E0104) — the label itself stays HIT; replay summaries count it in a separate `HIT?` column and print `*** HIT?` per record |

"Same diagnostic" for pairing means same code+message (`ita`) or same message
(`srb tc`), matched nearest-line-first, so a chronic diagnostic whose line
drifted is never mistaken for a silenced one. Whether the parent diagnostic
landed on the fixed line ±3 is recorded separately (`line_match`) — the
"ideally" of the spec, never part of the label. `class_match` is the same
kind of annotation: subject failure vocabulary (NoMethodError/NameError →
E0101/E0104, ArgumentError/wrong arity → E0102/E0103) checked against the
silenced diagnostic's code, `HIT?` in the summary when it does not match.

Precedence when several apply: NOISE > HIT > UNRELATED > MISS, and the
per-file detail keeps the silenced evidence visible under a NOISE label.

## Why this is the metric

The launch bar ("beat Sorbet on true errors found on a main snapshot") is a
dead-path metric. A pair benchmark measures the thing that matters: at the
parent revision the checker SHOULD scream (the bug is real, it became a fix),
and at the fix revision it MUST be silent — on that file, for that
diagnostic. HIT rate is recall on real bugs; NOISE rate is the false-positive
floor; both are measured on the same pairs, and `srb tc` runs on the same
pairs when the repo carries a `sorbet/` directory.

## Usage

```sh
# 1. clones (blob:none keeps it sane; worktrees fetch blobs lazily)
git clone --filter=blob:none https://github.com/rails/rails \
  ~/Sites/temp-files/bug-replay-corpora/rails
#    ... same for mastodon/mastodon, discourse/discourse

# 2. mine (300 candidates per repo, newest first; --signal narrows, e.g. --signal message)
ruby scripts/bug-replay/mine.rb ~/Sites/temp-files/bug-replay-corpora/rails rails

# 3. replay the newest 40 per repo (ita), srb on the same pairs when present
./scripts/bug-replay/replay.sh --limit 40

# prove the instruments (judge + miner), synthetic only
./scripts/bug-replay/selftest.sh
```

`replay.sh` exits 0 when at least one repo was judged, 1 on a hard failure,
and 2 when every repo was SKIPped (clone or candidates absent — each named on
its own SKIP line), mirroring `gauntlet-gates.sh`. Worktrees live under
`~/Sites/temp-files/bug-replay-worktrees/<id>/<sha>` and are reused when
present; they are never deleted unless `--cleanup-worktrees` is passed (and
then only once every selected record that uses the sha is done). The build is
part of the run: replay.sh runs `cargo build --release` and checks the exit
code before any capture, because evidence from the binary proves the binary.

## Adding a signal

Signals live in `mine.rb` in priority order — `message` (commit subject or
merged-PR title matches the Ruby failure vocabulary), `rescue-removal` (the
diff removes a `rescue NoMethodError|NameError|ArgumentError`),
`single-token-edit` (a line whose ONLY change is a method-name rename or one
positional argument added/removed). `--signal <list>` (comma-separated) mines
only the named signals; the default stays all signals. Records with identical
(path, removed, added) hunk sets — a merged PR yields both its merge commit
and its PR tip — are deduped to one: the descriptive (non-`Merge`-subject)
record wins, else the newest, and the dropped `fix_sha` is recorded under
`also` in the kept record. To add one:
1. Add a detector over the already-parsed commit (`c[:files]`, `c[:hunks]`)
   in `process_commit`, below the existing ones — a hunk predicate or a
   subject regex, nothing else.
2. Extend the `signal =` priority chain.
3. Add the new shape to `selftest.sh`'s miner section (one commit that fires
   it, one negative control that must stay silent) — two-sided proof lives
   next to the instrument, never only in a PR message.

Production scope is `app/ lib/` plus the Rails gem dirs, with `test/` and
`spec/` excluded by pathspec AND re-filtered per path — a test fix is not a
production bug.

## Validating a pair (human step)

For every HIT and every NOISE, a human validates by reading the code:

1. Open `results/<id>.jsonl`, find the record (`fix_sha`, `subject`).
2. `git -C <clone> worktree add --detach /tmp-work <fix_sha>` — or read the
   diff: `git -C <clone> show <fix_sha> -- <path>`.
3. For a HIT: confirm the removed lines really contained the bug the
   diagnostic describes (wrong method, wrong arity, missing constant), i.e.
   the parent-side diagnostic was a TRUE positive.
4. For a NOISE: read the flagged line at the FIX revision. If the code is
   actually correct, the diagnostic is a false positive on fixed code — the
   finding this project cares most about; file it against the checker, do
   not tune this harness to hide it.

This harness only collects and classifies; it never declares a HIT
"confirmed". Human reading is the proof.

## Honest limitations

- Message-based mining has recall bias toward well-described commits.
  Squash-merge repos with rich PR titles mine well; repos with "fix" subjects
  only contribute through the structural signals (rescue-removal, rename).
- Silence at the fix is necessary but not sufficient: a HIT can be a
  diagnostic that disappeared for unrelated reasons (the file moved, the
  class became dynamic). The record carries the diff to check exactly that.
- `signal: "single-token-edit"` pairs are heuristic: a rename-shaped diff may be a
  refactor, not a bug fix. The replay judgment sorts it out (most such pairs
  land MISS/UNRELATED, which costs budget, not correctness).
- Pathspec scope decides everything: a bug fixed in a directory outside the
  production scope is invisible to this benchmark.
- `srb tc --typed true` judges a Sorbet world (`sorbet/` RBIs) that the
  repo's own bundle provides; a missing-install worktree can produce srb
  errors unrelated to the pair — those records carry `srb_status` and are
  excluded from the head-to-head rather than counted.
- Mining caps (300 candidates/repo, hunk/line/file truncation flags in each
  record) bound record size; truncated records say so in-band.

# DOX framework — itaruby

- DOX is a self-documenting AGENTS.md hierarchy installed here (OMP-focused).
- Every agent must follow DOX instructions across any edits.

## Project

itá means "stone" in Tupi. itaruby looks at the stone and says whether it is a
real ruby: a Ruby type checker written in Rust, inference-first (in the style of
ty/Pyrefly, not Sorbet), with a real-time LSP server. Workspace of 4 crates
(`crates/itaruby_syntax` — parsing via ruby-prism; `crates/itaruby_semantic` —
index/types/checking as salsa queries; `crates/itaruby_server` — LSP;
`crates/itaruby` — the `ita` binary with `check` and `server`). Entry points:
`crates/itaruby/src/main.rs` (CLI) and `crates/itaruby_server/src/main_loop.rs`
(LSP).

**The project's bar: itaruby is the typechecker add-on for ruby-lsp, done
right.** The Shopify ruby-lsp has no type system (its own docs point users to
Sorbet/Steep) and its roadmap asks for "Allow the Ruby LSP to connect to a
typechecker add-on" — an aspiration, not an implementation. The gem in
`scripts/ruby-lsp-itaruby/` is that add-on; the VS Code extension in
`scripts/vscode/` is a reference client, never the main channel. Sorbet is the
precision adversary: itaruby is benchmarked against it on public Rails apps
(rails/rails, mastodon, discourse, gitlab-foss) on both speed and true errors
found, and the launch bar is winning on both.

## Core Contract

- AGENTS.md files are binding work contracts for their subtrees.
- Every meaningful change requires a DOX pass before the task is done: update
  the closest owning AGENTS.md when a change affects purpose, scope, ownership,
  contracts, workflows, constraints, or this index. Remove stale text
  immediately. Small no-behavior edits may leave docs unchanged — the pass
  still happens.
- Rules live in exactly one owning file. Child docs never restate parent
  rules. If two rules conflict, fix the docs in the same change —
  contradictions are bugs.
- Lessons learned in-session become rules with a date and a marker:
  `(learned YYYY-MM-DD, binding)`.

## Global Contracts

- **Invariant #1 (inviolable):** `Ty::Unknown` NEVER produces a diagnostic. A
  false negative is acceptable; ONE false positive on the benchmark corpora is
  a failure. Navigation follows the same rule: no-answer is acceptable, a
  wrong answer never is.
- **Fail-closed everywhere:** where itaruby cannot prove, it stays silent and
  the gap becomes roadmap — never a diagnostic (binding, 2026-08-24).
- A code path that turns silence into an error must never read absence of
  evidence as evidence of absence (learned 2026-08-24, binding): a gate that
  finds no signal on an upward search (e.g. no Gemfile discovered within N
  levels) fails closed to "couldn't tell, treat as present" rather than
  "confirmed absent, diagnose" — an unresolvable result is never license to
  accuse.
- The private benchmark corpora (corpus-a, corpus-b, corpus-c) are sacred:
  their expected error sets are recorded in `scripts/corpus-baseline.txt` as
  SHA-256 hashes only — never a path, class, method, table, or column name in
  clear text. Any new diagnostic on a corpus must be proven a true positive by
  reading the flagged code, or the change is reverted. Ratchets turn false
  positives into contracts: a wrong diagnostic that enters a baseline stays
  green forever (learned 2026-08-20, binding).
- The corpora are company code and each lives on exactly one machine — never
  rsync/copy a corpus between machines. Every corpus is READ-ONLY, always.
  Their real paths live only in the gitignored `scripts/corpora-local.txt` of
  each machine that hosts them.
- The secrecy wall above covers five surfaces, all audited: versioned file
  content; commit messages; PR titles and bodies; PR/issue comments and
  reviews; and tool-generated artifacts that get versioned (learned
  2026-08-20, binding). Names that only exist in a client's code never enter
  this repository; public gem API names are always fine.
- software-factory is alive (`sf check` in the pre-commit hook). NEVER loosen
  `.software-factory/policy.yaml` or the ratchet — a rule that blocks you is
  a finding about your change, not about the rule. That sentence used to be
  the only thing enforcing it; since 2026-08-26 `L2.POLICY_ONLY_TIGHTENS`
  and `L2.FACTORY_CONFIG_IS_LOCKED` make a loosening fail the build instead
  of relying on a reader (learned 2026-08-26, binding: a rule that lives
  only in prose is a rule the next tired agent edits around).
- A generated file says "do not hand-edit" and, until 2026-08-26, nothing
  stopped one. `L2.GENERATED_FILES_ARE_LOCKED` hash-locks the four generated
  declaration files; `declarations/gems.rbi` is deliberately outside that
  scope because it is hand-curated by the entry rule in its own header.
  `L2.DERIVED_ARTIFACTS_MATCH_THEIR_SOURCE` stays OFF with its reason in
  policy: every generator here reads state this repository does not own — a
  pinned Ruby version, an external checkout, or a private corpus — so
  running it in CI would compare against another machine's world.
- Zero clippy warnings and zero build warnings are the contract going
  forward: a tolerated warning never stays small (measured 2026-08-24: the
  workspace carried 58 clippy warnings while CI could not run). The bar now
  lives in `[workspace.lints]` rather than in a CI flag, so a local
  `cargo clippy` and the `hazards` job cannot disagree; it is
  `clippy::pedantic` denied and `unsafe_code` forbidden. The repository
  contains zero `unsafe` — the one that existed, an `extern "C" geteuid` in
  a test, was replaced by measuring the precondition it was proxying.
- A ratchet entry that freezes a whole LANGUAGE is the rule turned off with
  extra steps (learned 2026-08-26, binding, measured): `policy.yaml` claimed
  `L6.PERFORMANCE_REGRESSION_IS_GUARDED` and `L6.DATA_RACES_ARE_DETECTED`
  were enabled while `ratchet.yaml` froze both for `rust`, which is every
  line in this repository. Both freezes are gone and both rules are now
  satisfied by real tooling (criterion + `scripts/perf-gate.sh`; a nightly
  ThreadSanitizer job). Read a ratchet entry's SCOPE, never just its
  presence: `- rust` and `- path/to/one.rs:hash` are not the same kind of
  debt.
- Tracker: GitHub Issues on this repository. A local beads/Dolt database MAY
  exist per machine for agent-local planning memory, but `.beads/` is
  gitignored and never versioned here.

## Rules of proof — every instrument, every time

- Two-sided proof (learned 2026-08-20, binding): any new instrument (test,
  lint, gate, grep pattern) is delivered only after proving both sides — it
  fires under mutation (the real defect triggers it) AND it stays silent on
  correct content — and the probes proving each side live next to the
  instrument, never only narrated in a PR message.
- The mutation must re-emit the exact historical diagnostic to prove anything
  (learned 2026-08-25, binding): a mutant that only moves a counter or a
  bucket tally is blind.
- The lead re-runs every bead's mutant personally; subagent claims of
  "mutation caught" are inputs, not evidence.
- A rule that reads a DIRECTION is inert without a base revision (learned
  2026-08-26, binding, measured): `L2.POLICY_ONLY_TIGHTENS` compares the tree
  against a revision, so a plain `sf check` reports it green while it judges
  nothing. Proof: disabling `L1.COMPLEXITY_CEILING` in policy passed
  `sf check` and was caught only by `sf check --changed`. Both the
  pre-commit hook and gate 0 now run the `--changed HEAD` pass as well. The
  general form: when a rule's own text says "compared with", find out what it
  is being compared with before believing its green.
- Read a ratchet entry's SCOPE, not just its presence (learned 2026-08-26,
  binding, measured): `- rust` freezes every line in this repository and is
  the rule switched off with extra steps; `- path/to/one.rs:hash` is one
  known site. Two entries here were the first kind and read as ordinary debt
  for months.
- Ratchet slack is debt, not margin (learned 2026-08-21, binding): any anchor
  that measures a corpus below its declared ceiling tightens the ceiling in
  the same commit.
- A corpus baseline that does not pin the corpus REVISION cannot tell
  detection drift from content drift (learned 2026-09-18, binding, measured):
  gate d went red on corpus-a and corpus-b with 30 new `E0104` sites, and a
  bisect of three release binaries — the sha the ceilings were anchored on,
  the previous `origin/main`, and the head under test — produced byte-identical
  diagnostic sets, so the code had changed nothing; the corpora had advanced 71
  and 38 commits. Every ceiling and hash set now records the corpus sha it was
  measured against (`scripts/corpus-baseline.txt`), and gate d prints each
  corpus's current short sha next to its verdict.
- Evidence captured from `./target/release/ita` proves the binary, not the
  code (learned 2026-08-22, binding): every capture follows a
  `cargo build --release` whose exit code was checked, or comes from the gate
  run itself.
- Re-measure the number that motivated a bead before writing code against it
  (learned 2026-08-22, binding): bucket numbers in docs age; capabilities
  shipped later eat levers before their beads are born.
- A corpus SKIP on a machine that SHOULD host that corpus means the path
  rotted, not that the machine lacks it — the path is the suspect (learned
  2026-08-24, binding).
- A test whose outcome depends on the machine's tempdir layout is luck with
  a name (learned 2026-08-24, binding): build the symlink/mount shape on
  disk, or the test decides differently per machine.
- Isolated-subagent work the lead cannot cherry-pick by sha does not exist
  (learned 2026-08-25, binding): on any "merge failed" notice, hunt the
  sha/branch immediately, before anything else.
- A diff key that drops the column cannot see two same-code diagnostics on
  one line (learned 2026-08-26, binding): keyed on `path:line:code`, a line
  carrying both a dying diagnostic and a surviving one shows the surviving
  constant as falsely silenced when names are attributed through the key.
  Attribute by name with the column in the key, and count silenced sites
  from the message side, never from key survivors.
- A blind mutant means the suite lacks a control, not that the code is safe
  (learned 2026-08-26, binding): add the fixture that would catch it, prove
  the mutant fails it, and only then count the mutant as covered.
- A declaration that RESOLVES a name can be a regression, not a gain
  (learned 2026-08-26, binding): every suppression path keyed on "this
  ancestor never resolved" (`unresolved_ancestors`,
  `apply_undeclared_namespace_reopenings`' `by_path` miss) silently stops
  firing the moment a curated declarations file names that ancestor. Bead
  ita-dpg.1's pack made corpus-c go 179 -> 2746 E0104 that way. Every such
  path must read `DeclaredExternal` as still-unknown — `by_path` presence is
  not the project declaring it — and any new declarations entry is measured
  against the corpora BOTH directions, never only for the warnings it kills.
- A generated declarations file inherits its source's reopenings (learned
  2026-08-26, binding): a gem's `.rbs`/`.rbi` freely reopens core classes to
  declare its own extensions, and every declared path is force-opened by
  `merge_declared_fragment` — so harvesting one wholesale turns off
  conclusive E0101 on those receivers project-wide. Any generator excludes
  the names this checker already models, and a test on the generated file
  proves the filter survived regeneration.
- A control asserting a name is ABSENT has an expiry date (learned 2026-08-26,
  binding): two wave-10 slices cut from the same commit were each green alone
  and red together — one re-picked this control's undeclared example, the
  other declared exactly that name from its own measurement. Whenever a
  declarations file grows, re-check every absence-based control mechanically
  against both files, and say in the fixture that the name will decay.
- Prefix fallback is a suffix grep wearing an API (learned 2026-08-26,
  binding, extending bead ita-y0s): a nested member is resolved by EXACT
  path, never by "some declared ancestor is a prefix of it". Nothing
  structural separates a member missing from an old pinned RBS version from
  a name that never existed, and when there is no signal this repo takes the
  false negative over a blanket suppression. See `Removed — do not
  reintroduce`.
- Rank a declaration candidate by EXACT PATH, never by top-level segment
  (learned 2026-08-26, binding, measured): ranking the residual by segment
  claimed 2865 addressable sites where the exact-path truth was 341, an 8x
  overcount, because a directory declaring `Rails` says nothing about which
  `Rails::*` member is missing. Same family as the constant-visibility bead,
  where segment-level matching invented a whole false-positive bead: the unit
  of measurement decides the conclusion.
- Directory-exists is not yield (learned 2026-08-26, binding): a gem may be
  in the collection, be listed in the generator, and still yield zero entries
  because a filter upstream already owns its namespace (`rake`, filtered by
  the stdlib inventory). Confirm the entry count after regenerating, not the
  directory before.
- A decision taken against one corpus expires (learned 2026-08-26, binding):
  the recorded reasoning for excluding `Boolean`/`ID` — app-level aliases
  defined in an initializer — was re-checked against 224 live sites and is
  FALSE there; no alias exists in that project and the names are real gem
  constants reached by ordinary Ruby ancestor lookup. Those sites had been
  carried as "correctly warned" for several waves. Consistent with the old
  reasoning is not the same as proven by it.
- A receiver-blind softening MUST key on the method name (learned 2026-08-26,
  binding, measured twice): keying a dynamic-mixin softening on "some mixed
  module has `method_missing`" silenced 100% of two corpora's errors, because
  a real project always contains some unrelated dynamically-mixed
  `method_missing` target and one of them poisons every lookup on its track.
  The name-keyed form closed the intended cluster with zero collateral. When
  a mechanism cannot see the receiver, only the name can carry the proof.
- The anchor worktree needs the corpora declaration copied in (learned
  2026-08-26, binding): `scripts/corpora-local.txt` is machine-local, so a
  fresh detached worktree has none and the corpus gate SKIPs every corpus
  while still printing `RESULT: PASS (incomplete)`. That is the wave-9 trap
  in a new shape — absence of proof reading as the expected absence. Copy the
  file from the machine's own checkout into the worktree before trusting an
  anchor, and check the `corpus gate proved:` line names what you expected.
- Same-wave work touching the same file needs an explicit disjoint-region
  note in the brief (learned, binding): two same-wave workstreams once
  edited the same function in the same file and produced a real merge
  conflict resolved by hand; the lead still reviews merge order regardless.
- Ticket text that contradicts an already-proven allowlist loses (learned,
  binding): the allowlist wins and the divergence is recorded — when a
  ticket's acceptance criteria assumed a method should behave differently
  than an allowlist already proven correct by mutation, the implementation
  followed the allowlist, not the ticket's stale assumption.
- A scratch-tree conclusion is re-derived from a pristine tree before any
  code is written against it (learned 2026-09-03, binding, measured): a
  "walker-order artifact" — the checker silent on a class's own method
  while byte-identical bodies in sibling files fired — was measured on a
  scratch mutated by sixteen accumulating in-place edits, and the file
  being judged already carried the fix (`@template_object.request`) plus a
  leftover `end end end)))` trailer. Rebuilt from the parent revision's
  exact bytes, every shape fired where the scratch was silent. An
  observation whose producing tree cannot be reconstructed byte-for-byte
  is a hypothesis, not a measurement: record the sha and the diff of every
  scratch before reading its output.
- A gate transcript read through a filter is not the gate's verdict (learned
  2026-09-21, binding, measured twice in one night): two full
  `singleton-mutants.sh` runs "ended" right after MUT-Q, and the matrix read
  as complete-minus-two. The harness was right both times — my summary pipe
  hid `line 137: $6: unbound variable`, because MUT-R's invocation had been
  added with its rationale argument missing, and `set -u` aborts the script
  there. Count the mutants in the raw log against the harness's declared
  list before believing a transcript, and read the tail of the raw log, never
  a grep of it. The same session's tautological-prefilter defect (a `def `
  needle searched over a span that BEGINS with `def `, so every body passed)
  was caught only because MUT-F went BLIND — the blind-mutant rule above
  earning its keep.
- A `mutant`-style invocation is code and gets checked like code: a missing
  argument does not fail the mutant, it kills the run before the mutant
  exists.
- The instrument that PRODUCES the evidence gets the same two-sided
  treatment as the code it judges (learned 2026-09-17, binding, measured
  three times in one file pair): every rule above governs checks that read
  evidence, and nothing governed the scripts that MAKE it. All three of
  these were live and silent. `gauntlet-gates.sh` printed
  `FAIL cargo build` and then ran every binary-consuming gate against
  whatever `./target/release/ita` happened to be on disk — proved by a
  probe whose stale binary recorded its own execution after an injected
  build failure. `replay.sh` appended each run into one
  `results/<id>.jsonl`, so the file carried 33 lines for 10 unique pairs
  with `rails-1` holding verdicts from two different days — a 2026-09-03
  MISS reading as today's recall. And it built into cargo's global target
  dir (`~/.cargo/target`, set by this machine's config) while executing
  `$ROOT/target/release/ita`, so "captures prove the binary" was measuring
  a binary the checked build never produced. The family: an evidence
  producer that can lie makes every gate downstream of it decorative.
  `scripts/instrument-mutants.sh` now re-injects each defect as a mutant
  and demands it reproduce, and runs as gate c2b.
- A probe that fails for the wrong reason proves nothing, and the wrong
  reason is usually the fixture (learned 2026-09-17, binding, measured):
  the positive-control run of the harness above — a deliberately blind
  copy that must be caught by its own `INVALIDO-cmp` guard — failed all
  three cases instead of one. The copy had been written outside
  `scripts/`, and since the harness derives `ROOT` from
  `dirname $0/..`, it never found the files it was mutating. The guard's
  green would have been read as "the guard works". Always check WHICH
  case accused and that the others stayed green; a control that fails
  everywhere is a broken fixture wearing a passing verdict.
- An agent working in a worktree still resolves RELATIVE paths against
  its own session directory (learned 2026-09-17, binding, measured): a
  subagent briefed to work only in `~/Sites/worktrees/itaruby/<slug>`
  wrote four edits into the MAIN checkout instead, because its edit tool
  resolved `crates/...` against the session cwd while every `bash` step
  had `cd`'d into the worktree. Nothing failed — the edits applied
  cleanly, to the wrong tree, on top of another agent's uncommitted
  work. Any tool call that WRITES from inside a worktree task uses an
  absolute path, and `git rev-parse --show-toplevel` is the cheap check
  before the first edit.
- An evidence producer must write where the reader looks, and
  `--target-dir` does not move all of it (learned 2026-09-17, binding,
  measured): `scripts/perf-gate.sh` ran `cargo bench` and then read
  `target/criterion/<id>/new/estimates.json`, so on this machine — whose
  `~/.cargo/config.toml` sets a global `build.target-dir` — every row
  reported "produced no estimate" while the bench itself ran fine. The
  flag alone was not the fix: criterion resolves its output root from
  `CRITERION_HOME`/`CARGO_TARGET_DIR`, so the build moved and the
  measurements stayed behind. Both are now set explicitly. Same family
  as the build/capture mismatch above; this one failed loudly instead of
  greenly, which is the only reason it was cheap.
- Two worktrees of this repo share one cargo target dir on this machine,
  and that makes a green or red run a coin flip (learned 2026-09-17,
  binding, measured twice in one session): `~/.cargo/config.toml` sets a
  global `build.target-dir`, so while another agent built the same crate
  from the main checkout, (a) `cargo test --test operand_types` in a
  worktree failed with `no E0108_OPERAND_TYPE_MISMATCH in the root`
  against a `lib.rs` that exports it — resolving the OTHER tree's
  `rmeta` — and passed immediately with `CARGO_TARGET_DIR=$PWD/target`,
  and (b) `json_format.rs`'s tempdir, keyed only by test name under the
  shared `CARGO_TARGET_TMPDIR`, was deleted mid-test by the identically
  named test in the other tree (`write` panicked with `NotFound` on a
  directory it had just created). Any test/bench/mutation evidence from
  a worktree pins `CARGO_TARGET_DIR` to that worktree, and a lone red
  from a shared-dir run is re-run pinned before it is believed.
- A perf ceiling measured on a loaded machine is not a measurement
  (learned 2026-09-17, binding, measured): with other agents building on
  the same host (load average ~13), the PRISTINE `HEAD` tree measured
  `check/project_index` at 21.7 ms against a 7.04 ms ceiling and
  `check/index_and_check_all` at 105 ms against 43.4 ms — a 3x and 2.4x
  "regression" produced by contention alone. A perf verdict is only
  reportable when the same window also measures the parent revision, and
  the two are compared to each other before either is compared to the
  ceiling.
- A disambiguated anchor is a NEW anchor and needs its own match count
  (learned 2026-09-18, binding, measured): the singleton track added two
  nested `Visit` impls to `index.rs` whose own
  `ruby_prism::visit_call_node(self, node);` lines contain M14's 8-space
  needle as a substring, so the bare anchor went from one match to three.
  The replacement shipped with the track named `self.note_refinement(node);`
  as the line above the recursion — but `note_opaque_eval` and
  `note_injection` sit between them, so the new needle matched ZERO times
  and the mutant was never injected. `INVALIDO` caught it, forty minutes
  into gate c1, on the integration run. Counting a candidate needle with
  the harness's own semantics (`src.count(needle)`) costs three seconds:
  every anchor edit does it, and no anchor edit is believed until its leg
  has been re-run once. Since 2026-09-19 that count is a script —
  `scripts/mutant-anchors` counts every needle in every `scripts/*-mutants.sh`
  against the working tree (111 anchors across ten harnesses), and gate c1 runs
  it BEFORE the families, so a dead anchor costs two seconds instead of the 26
  minutes of family it cost the day wave 2's neighbour insertions killed M14 and
  M15. It refuses to run while a harness holds the tree, because a mutant is
  what it would otherwise measure.

## Engineering History

The per-bead engineering log (waves 1-10, sections A through J — editor and
protocol integration, inference depth, schema-as-declaration, coverage-census
methodology, precision fixes, repo hygiene, the symlink incident, the launch
campaign, the ActiveRecord-API saga, and cross-cutting lessons) lives in
[`docs/engineering-history.md`](docs/engineering-history.md). It carries the
narrative and the measured numbers behind each fix — every rule in it worth
enforcing again has already been promoted into Global Contracts or Rules of
proof above; the child file is record, never a second copy of a contract.

## Gates — and where each runs

The gates are one script, not model calls: `scripts/gauntlet-gates.sh` runs
them all and returns an exit code — `0` all green, `1` a gate failed, `2`
green but at least one declared corpus could not be checked on this machine
(the transcript names which).

| Gate | Needs | Runs on |
|---|---|---|
| `sf check` (software-factory, gate #0) | only the repo | **any machine** |
| Gate 0b — MRI interpreter (`ruby -v` ≥ 3.x; a missing ruby is fine, the MRI legs skip) | only the repo | **any machine** |
| `cargo test --workspace` | only the repo | **any machine** |
| Mutation probes in `testdata/` | only the repo | **any machine** |
| Per-fix source mutants (gate c1): `scripts/const-missing-mutants.sh` (E0104 `const_missing` suppression) and `scripts/operand-types-mutants.sh` (E0108 + the refinement, eval-body and name-keyed pollution decisions, mutants M1a/M1b/M2–M7/M13–M47, run against both the `operand_types` and `core_conclusive` suites with `--no-fail-fast`) and `scripts/singleton-mutants.sh` (the singleton track: receiver-spelling attr filing, class_attribute predicate, thread variants, the lock-gated `any_instance` softening, the `_exec` prefilter family, the concern-edge gate on the `class_methods do` harvest, the `gem_namespace_key` camelize key, the `class << self` track routing for `define_method`/`alias_method`/`alias`, the literal def-body filing with its fail-closed gates on an instance body and on a foreign receiver, and the two sides of the `send(:define_method, ...)` unwrap; mutants MUT-A..MUT-P, run with `--no-fail-fast`) and `scripts/mixin-attribution-mutants.sh` (the attributed-mixin family: the `method_missing` gate, the literal-constant receiver, both ternary arms, the receiverless project call, the interpolated-`def` harvest being called and its names being filed, the eval call's receiver deciding where they land, and the instance-only track filter that keeps an `extend` edge from silencing instance lookups; mutants MUT-1a/1b/1c, MUT-2a/2b/2c, MUT-3a/3b/3c) and, from fase A/onda 2, five families on the same terms — `scripts/lazy-load-mutants.sh` (bead B: the `run_load_hooks` base openness), `scripts/extended-hook-mutants.sh` (bead H: what a `self.extended` hook installs on its extender), `scripts/guard-narrowing-mutants.sh` (bead C: the two predicate-proven shapes), `scripts/asserted-raise-mutants.sh` (bead E: the asserted-raise subject span) and `scripts/rebindable-guard-mutants.sh` (bead F: the guard moved above the lookup dispatch) — one decision removed at a time, each accused by a NAMED test, source restored byte-identical with `cmp`, `INVALIDO` when an anchor no longer matches | only the repo | **any machine** |
| `scripts/unwrap-gate.sh` — every `unwrap()` in production source is a prism downcast | only the repo | **any machine** |
| `scripts/instrument-mutants.sh` — the evidence producers themselves (gate fail-fast, replay run isolation, replay build pin): each defect re-injected as a mutant, shipped scripts proved clean | only the repo | **any machine** |
| `scripts/perf-gate.sh` — criterion medians vs `scripts/perf-baseline.txt` | only the repo | **any machine** (tight ceiling on a dev machine, loose one under `CI`) |
| Corpus diff vs `scripts/corpus-baseline.txt` | per declared corpus, checked individually where its path is mapped in `scripts/corpora-local.txt` | corpus-a/corpus-b: **only `work`**; corpus-c: **only `m5`** |
| Navigation oracle vs `scripts/navfixture/oracle.jsonl` | only the repo | **any machine** |
| `scripts/public-gate.sh` vs `scripts/public-corpora.txt` + `scripts/public-baseline/<id>.jsonl` | a clone of each declared public repo under `$HOME/Sites/temp-files/public-corpora` (never auto-cloned; `PUBLIC_GATE_CLONE=1` opts in); error sets are **drift detectors, unaudited** until `scripts/public-baseline/README.md`'s ledger has a verdict per family | **any machine with the clones** |
| `scripts/inference-gate.sh` — the inference bench (`scripts/inference-bench.jsonl`), guarded by `scripts/inference-bench-selftest.sh` | only the repo; `ruby` for the ground truth, `srb` at the pinned version for the comparison leg (missing/mismatched → that leg skips) | **any machine** |
| `scripts/gate-digest` — the whole run as ONE compact JSON at `target/gauntlet/digest.json` (per-gate status + one-line reason + numbers, and for a FAIL the artifact path and line numbers), written on every exit path of `gauntlet-gates.sh` including the early ones; a reporter, never an exit code. Guarded by `scripts/gate-digest-selftest.sh` (gate c2b): four fixture gauntlet dirs under `scripts/gate-digest-fixture/` reported exactly, five cmp-guarded mutants each accused by its own case. Two rules it cannot break: PRIVATE corpus artifacts (`corpus-*`) yield counts, hashes and line numbers only — never a byte of content — while public corpora may show `path:line` of NEW/GONE lines; and a PASS whose evidence file is missing or empty is reported `FAIL "artifact absent"`, never PASS. Read this INSTEAD of the ~115 MB `target/gauntlet/` holds (measured 2026-09-18: `public-discourse.txt` alone is 440 KB, ~110k tokens; an all-green digest is 1791 B) | only the repo | **any machine** |
| `scripts/gate-triage` — routes a finished run to its next action, reading `target/gauntlet/digest.json` ONLY: deterministic rules over digest features (perf red without a parent-revision measurement, corpus rev drift, corpus error drift, artifact absence, public drift, loaded host), plus four optional Jev judgments (`typesafe/jev-1.13`, ~0.6 s and ~US$0.00005 measured 2026-09-18). Three rules it cannot break: ADVISORY ONLY — it never gates, the gauntlet's exit codes stay untouched, transport failure prints one advice line and exits 4 fail-open; DIGEST-ONLY STATE — the state sent to the model is a proven pure function of `digest.json` (the secrecy wall travels with the digest; no model call ever sees artifact content); JUDGMENTS ROUTE, PROOF STAYS WITH THE READER — a Jev answer may annotate or order an action, never create, dismiss, or block one. Guarded by `scripts/gate-triage-selftest.sh` (gate c2c, offline by construction): eight fixture runs routed exactly (green routes NOTHING), ten cmp-guarded mutants each accused by its named guard, unknown answer keys rejected (exit 2), a byte ceiling on the green state (1723 B measured) | only the repo | **any machine** |

Any model judgment inside this pipeline — today exactly one, `scripts/gate-triage`'s
Jev battery — ROUTES and never gates: the gauntlet's verdicts stay deterministic, a
probabilistic answer may annotate or order a repair but never create, dismiss, or
block one, and a transport failure is fail-open (learned 2026-09-18, binding). Any
future model call in this repository inherits the same three rules and the same
two-sided proof, and its state is built from the digest or from files this
repository already versions in the clear — never from a private corpus artifact.

The public corpora are the ONLY corpora whose diagnostics are versioned in the clear, because they are public — the secrecy wall still covers the three private corpora unchanged.

The inference bench (`scripts/inference-bench-README.md`) answers a different
question from every corpus gate: on twelve self-contained, gem-free fixtures,
does itaruby find real bugs with ZERO annotations and stay silent on dynamic
code that is correct at runtime? Its ground truth is MRI — every `accuse`
fixture is executed and must really raise on the blamed line, every `silent`
fixture must really exit 0 — so a checker is never judged against another
checker's opinion. It is NOT a parity benchmark and must never be quoted as
one: no gems, no Tapioca RBIs, and no itaruby curated declarations are in
play, so it measures neither tool on a real app. Where Sorbet reports on
code that runs clean, the ONLY licensed claim is "Sorbet cannot prove this
without an annotation", and it is licensed only by the `with_annotations`
leg — the same program, sha-checked, plus the sig/RBI a human or Tapioca
would write, measured clean. Rows without that leg are reported, never
claimed. Both directions are published: the two cases where Sorbet proves a
bug itaruby misses (`extend_singleton_typo`,
`included_hook_class_method_typo`) are rows in the ledger, and the one live
invariant #1 violation the bench found (`const_missing_namespace`, E0104 on
a namespace defining `self.const_missing`) was fixed in `check_const_ref`
the day it was found — suppression-only, with the no-hook and
sibling-namespace controls in `testdata/const_missing/` proving the warning
survives everywhere the hook does not reach.

A silence row needs a POSITIVE CONTROL or it proves nothing (learned
2026-09-17, binding, measured): silence on correct dynamic code and silence
from a checker blind to the whole surface are the same observation. Each of
the bench's three dynamic-silence rows now carries a sibling fixture with a
certain typo planted; MRI raises on all three and itaruby is silent on all
three, so those rows license "does not false-positive here" and never
"models this pattern". The bench prints that limitation itself.

Reporting a singleton `NotFound` is NOT an additive low-risk change
(measured 2026-09-17, binding, reverted): the two gaps above look like pure
silence-to-diagnostic wins, and an implementation of exactly that added
703/36/4109 diagnostics to rails/mastodon/discourse — narrowed to explicit
receivers plus a did-you-mean near miss, still 216/0/12, and every sample
inspected was a real method (`SecureRandom.uuid`, `Kernel.rand`,
`mattr_accessor` writers, `class << self` accessors). A class object's
singleton surface comes from populations this index does not model at all
(`mattr_accessor`/`cattr_accessor` appear zero times in `index.rs`), so
absence from the index is not absence at runtime. Index those populations
first; until then the gaps stay open and characterized in
`crates/itaruby_semantic/tests/singleton_lookup.rs`.

Any claim quoted out of this bench carries its full scope or it is not made
(binding, 2026-09-17): **12** fixtures, sigil **`# typed: true`**, **srb
0.6.13437**, **zero gems / zero Tapioca RBIs / zero itaruby curated
declarations**, and **no timing claim** — this bench measures verdicts only,
the speed bar lives in `scripts/perf-gate.sh` and the public-corpus gate.
The bench prints that block itself (`--- claim scope ---`) with every number
derived from the manifest, and fails when rows disagree about the sigil or
the pinned version, because one sentence cannot then cover them.

The lint policy is not a gate script: it lives in `[workspace.lints]` in the
root `Cargo.toml`, so `cargo clippy` locally and the `hazards` job in CI
cannot disagree about what the bar is. `clippy::pedantic` is denied,
`unsafe_code` is forbidden (the repository contains none), and every
exception names its lint and its reason — five families allowed at the
workspace level, and a handful of `#[expect]` at call sites where the value
is provably bounded. `expect` rather than `allow` on purpose: a suppression
that stops being needed fails the build instead of quietly rotting.

Full proof today needs two machines — `./scripts/dev gates` on `m5` (proves
corpus-c) plus the anchor on `work` (proves corpus-a and corpus-b) — because
no single machine holds all three corpora.

## Multi-machine: corpora on both, full proof needs both

Machines (tailnet): `work` (100.86.40.27, corpora corpus-a and corpus-b) and
`m5`/`m5-de-ary` (100.88.227.102, the strong machine, corpus-c). SSH aliases:
`ssh m5-ary` from here, `ssh work` from there (pinned key, `IdentitiesOnly
yes`). The `work` host's own tailnet name is deliberately not written here: it
is named after one of the private corpora's owners, so spelling it out would
name a client through the back door — the same wall that keeps corpus ids
generic (learned 2026-08-26, binding). Resolve it from `~/.ssh/config`, which
is machine-local and unversioned.

The performance ceiling is per-machine in the same way the corpora are, and
is honest about it: `scripts/perf-baseline.txt`'s `dev` column is an `m5`
measurement, recorded with its date and conditions. `scripts/perf-gate.sh`
asks for the column via `PERF_COLUMN` and NEVER infers it — keying off `$CI`
was tried and is a trap, because agent harnesses and plenty of local shells
export `CI`, and the failure is silent in the wrong direction: a dev machine
quietly judged against the loose ceiling passes exactly the regression the
gate exists to catch (learned 2026-08-26, binding). Until `work` has a
measured column of its own, run the anchor there with `PERF_COLUMN=ci`; a
ceiling copied between machines is a fabricated measurement, and raising the
`m5` column to fit a slower machine is the loosening this repository forbids.

Anchor flow (run gates on `work` against an exact sha, in a disposable
worktree — never disturb the checkout another agent may be using):

```sh
ssh work 'cd ~/Sites/personal-team/itaruby && git fetch -q origin main && \
  git worktree add -f --detach ~/Sites/worktrees/ita-anchor-<slug> <sha> && \
  cp ~/Sites/personal-team/itaruby/scripts/corpora-local.txt \
     ~/Sites/worktrees/ita-anchor-<slug>/scripts/corpora-local.txt && \
  cd ~/Sites/worktrees/ita-anchor-<slug> && \
  PERF_COLUMN=ci PATH="/opt/homebrew/opt/ruby/bin:$HOME/.cargo/bin:$PATH" ./scripts/gauntlet-gates.sh; \
  cd ~/Sites/personal-team/itaruby && git worktree remove --force ~/Sites/worktrees/ita-anchor-<slug>'
```

The host checkout on `work` is `~/Sites/personal-team/itaruby` — the one that
holds `scripts/corpora-local.txt`. `~/Sites/itaruby` there is a different
checkout on another branch with no map, and following it silently SKIPs both
corpora while the transcript still ends in `PASS (incomplete)` (learned
2026-09-18, binding, measured). Read the `corpus gate proved:` line and
require it to name `corpus-a` AND `corpus-b` before calling an anchor green.

The split is over (2026-09-22): `aryrabelo/itaruby` (public) is THE
repository; `aryrabelo/itaruby-private` is an archive that receives nothing
new. On 2026-09-22 the 21 commits the public lacked were cherry-picked onto
it with their original messages after a mechanical secrecy audit (every
segment of the corpus path and of the anchor host's name counted across
messages and tree: zero new hits), and the trees were proved byte-identical
before each push. Until then the two repos shared no ancestor, `origin` on
`work` fetched nothing this repository committed, and the anchor failed
with `fatal: invalid reference: <sha>` — twice in one night before the
remote was read (learned 2026-09-21, binding, measured). `scripts/dev
anchor` fetches from `$ANCHOR_REMOTE`, default `origin`; on `work` the
`private` remote may still exist and is never the answer. Every commit
message, PR title and PR body is a public surface from the first byte — the
secrecy wall's five surfaces above are no longer "audited before sync",
they are live.

Explicit `PATH` because a non-interactive ssh shell does not load
`~/.cargo/bin` — without it `sf` and friends "don't exist" on `work` even when
installed (learned 2026-08-20, binding). The same trap one layer down, for
`ruby` (learned 2026-09-18, binding, measured): a non-interactive ssh shell
resolves `ruby` to `/usr/bin/ruby` 2.6, which cannot parse an endless method
definition, so gates a, c1 and g fail on the interpreter and not on the code —
identical run with brew's ruby 4.0.6 turns all three green, every other verdict
byte-identical. And `$(brew --prefix ruby)` does NOT reach it: `brew` itself
is not on a non-interactive ssh PATH, so the substitution expands to the empty
string and the PATH prefix becomes `/bin:...`, landing right back on
`/usr/bin/ruby` 2.6 — the anchor block and `scripts/dev` therefore hardcode
`/opt/homebrew/opt/ruby/bin` on `work` (learned 2026-09-21, measured: one
full anchor run wasted, gate 0b naming ruby 2.6.10 under a `PATH` that was
supposed to prepend brew's ruby). It is a PATH trap, not an ssh trap, and it
fires locally too
(learned 2026-09-19, binding, measured): a local `./scripts/dev gates` whose
environment put `/usr/bin` before mise's shims ran gate a on ruby 2.6, and the
red surfaced as `refined_integer_plus_silent.rb must run clean: syntax error,
unexpected '='` INSIDE a fixture — ~70 minutes into gate c1, with the two
mutant families that run the operand-types suite aborting on `baseline is not
green` (the harness's own guard, correct, against a baseline red for the wrong
reason). Gate 0b now names the interpreter before anything runs it (`PASS
ruby 3.x`, or a FAIL with the version and the path), so that verdict costs two
seconds instead of a mutation pass. On `m5` the 3.x ruby is mise's shim under
`~/.local/share/mise/shims`, and there is no brew ruby at
`/opt/homebrew/opt/ruby/bin` to reach for.

## Bootstrap on a new machine

```sh
git clone git@github.com:aryrabelo/itaruby.git ~/Sites/itaruby
cd ~/Sites/itaruby
git config core.hooksPath .githooks
cargo build --release
cargo install --git https://github.com/nicolasmelo1/software-factory --locked
```

If the machine hosts a corpus, also create `scripts/corpora-local.txt`
(gitignored) mapping corpus id to absolute path, one per line:
`corpus-c /path/to/the/corpus`.

`./scripts/dev setup` is the one-line idempotent equivalent and also prints
which gates this machine can run (see `scripts/dev` for the other verbs,
`gates` and `anchor`).

## Verification

```sh
./scripts/dev gates    # local gates; exit 2 = declared corpus(es) absent here (named in transcript)
./scripts/dev anchor   # gates on work, at the local HEAD sha
```

`anchor` checks out the local sha on `work` on purpose: an anchor run against
whatever the remote happened to have on disk is a false green. It requires the
commit to already be on a remote (exit 65 otherwise).

`ita check --dark-singletons=<file> <path>` runs the dark singleton census: a
measurement instrument (never a gate) whose JSONL records what the class-object
track WOULD accuse (`closed_notfound` — the residue) against the blocker
standing every other receiver down (`open(<reason>)`). Since the tail bead
(2026-09-20) a third verdict exists: `known_tail` — silent because the NAME is
the core bare-call tail (`core::kernel_bare_call_method`), generated into
`declarations/core_inventory.txt`'s `Class~method` section by
`scripts/gen-core-inventory.rb` (private harvest of
Kernel/BasicObject/Module/Class/Object, `initialize` excluded; `URI`/`pp`
hand-added as default-stdlib extensions invisible to `--disable-gems`, same
precedent as `gem`). It changes no diagnostic — proven byte-identical on
rails/mastodon/discourse — and doubles as the net for span bugs the
diagnostic path never renders (`on_char_boundary: false`).
`scripts/dark-singleton-summary` reads the JSONL and flags any residue site
still carrying a tail name (a wiring gap). Post-tail residue: 56/4/44 on
rails/mastodon/discourse (from 319/153/4389), the hand-auditable remainder.
The 2026-09-20 audit of that remainder (full table in
`scripts/public-baseline/README.md`) found 2 confirmed + 1 pending TRUE
positives — including a real discourse bug (`lib/color_math.rb:62`
`raise new RuntimeError(...)` raises NoMethodError instead of the intended
message) — and 98 populated sites across 9 named mechanisms: **the
class-object flip stays closed until those populations are indexed**, per
the same rule as the singleton-track gaps below. (2026-09-21, bead
ita-census: those were census RECORDS, inflated by the foreign-span ghost
emissions the instrument fix removed — the true distinct-site remainder
is 50/1/20; the family-level audit table stands, now with trustworthy
site-level attribution.) The census instrument's remaining known bug
(the duplicated-record/instance-leak note above) was that same foreign-
span emission and is FIXED by ita-census; the site-level audit can
proceed.

Per-run artifacts land in `target/gauntlet/` (gitignored).

### Removed — do not reintroduce

- The name `rubyt` (collides with an existing gem on rubygems; renamed to
  itaruby 2026-08-20).
- Committing `.beads/` or any tool-generated tracker export (the tracker is
  GitHub Issues in this repo).
- Any of the three private corpus names, anywhere in the tree (they live
  only in the private `itaruby-alpha` archive repo).
- Any on-disk result cache (learned 2026-08-26, binding — measured twice).
  Gem-source cache died in ita-k9j.1 (validation 33–58ms > re-parse
  24.9ms). A whole-project snapshot died on the phase probe
  (`measure/cache-phase-probe`): check dominates a run (66–72%),
  read+wire alone is 17–26%, so "did anything change?" costs ~26% of a
  cold run; and per-file invalidation is unsound in Ruby (any file can
  reopen any class — one edit can invalidate every diagnostic), so the
  only sound granularity is all-or-nothing with a near-zero hit rate in
  every real workload (CI runs once per push, the binary version is part
  of any honest key, local iteration is already served). The cache is
  the LSP: for repeated use, run `ita server` — salsa recomputes only
  what an edit actually invalidates.
- Prefix suppression of E0104 under a `DeclaredExternal` namespace (built
  and reverted 2026-08-26, bead ita-dpg.2). The idea: a reference whose
  longest declared ancestor prefix is force-open suppresses E0104, which
  would have closed ~800 measured sites (`Nokogiri::HTML5` 193,
  `GraphQL::Types::ID` 158, `Faker::Number` 106, `Stripe::*` 136, ...).
  Killed because it cannot distinguish those from `T::NotACuratedName`,
  which an existing control requires to keep warning: `T` is a curated
  multi-entry namespace structurally identical to every example gem. The
  root cause was then measured directly instead — neither `Nokogiri::HTML5`
  nor `GraphQL::Types::ID` exists in ANY version of `gem_rbs_collection`,
  so no resolution rule should have produced them. The honest fix is exact
  curated names, which is what shipped.
- Parsing a LITERAL eval body as a sub-program to narrow the eval-pollution
  mark (specified and measured 2026-09-18, never built). The idea: when a
  `class_eval`/`module_eval`/`instance_eval`/`eval` argument is a string
  literal, parse it with prism and scan the sub-tree so only the core
  classes it really reopens get marked, instead of standing every core
  class down. Measured with prism over the three public corpora before
  writing any Rust: of the sites that stand every core class down, only
  4 of 98 in rails and 46 of 123 in discourse have a literal body at all
  (mastodon has zero such sites), and both projects keep dozens of
  genuinely dynamic ones — 43 `eval(<local>)` plus 17 interpolated
  `class_eval <<-CODE` in rails, 31 `eval(<call>)` plus 15 `eval(<local>)`
  in discourse — so the project-wide mark survives either way and no
  corpus changes verdict. The mechanism (a recursive sub-program scan
  with a depth cap and a parse per literal) would have bought precision
  no corpus can observe.
  Two measurements from the same session carry forward, because the
  premise that motivated the work was itself wrong. First: E0108 was
  already dead on all three public corpora BEFORE the eval marks existed
  — a `p 1 + "s"` probe added to each clone, checked with the parent
  revision's binary, reports 0 E0108 rows on rails, mastodon and
  discourse — so the eval mark is not the binding constraint anywhere.
  The real ones are `core_mixin` (receiver-blind and project-wide;
  `Object.prepend(M)` in rails and discourse, plus `Integer.include` /
  `Float.include` in rails) and plain `by_path` reopenings of
  `Object`/`Kernel`/`Numeric`. Second: across all three corpora, 248
  methods are defined directly inside core-class reopenings and ZERO of
  them is an operator (`+ - * /`) or a coercion hook
  (`coerce`/`to_str`/`to_int`). So the lever that would actually move
  this capability is NAME-KEYED pollution — asking whether a reopening
  can touch the method the diagnostic depends on, the same lesson as the
  receiver-blind dynamic-mixin softening above — and not a better reader
  for eval strings. It would flip mastodon on its own; rails and
  discourse additionally need a decision about whether an unreadable
  `eval(<runtime string>)` should stand E0108 down at all, given that the
  checker already tolerates the identical unmodeled risk from every
  undeclared gem in the Gemfile. That decision is the owner's, not an
  implementer's.

- A blanket "was this touched?" and a keyed "was THIS name touched?" are
  different questions, and the cheap one answers neither honestly
  (learned 2026-09-18, binding, measured): E0108 spent three rounds
  reading the closed-world lookup's pollution signal, which stands a
  core class down when anything at all reaches it. Measured across
  rails/mastodon/discourse: 248 methods are defined directly inside
  core-class reopenings and ZERO is an operator or a coercion hook, so
  every one of those stand-downs was silence bought for nothing. The
  general form: before reusing an existing suppression signal, ask what
  question it answers, then measure how often its answer and the one you
  need disagree.
- Ruby's dispatch asymmetry, measured on 3.4.2 and not to be re-derived
  from reasoning (learned 2026-09-18, binding): for `<literal> <op>
  <literal>`, only the RECEIVER's own class can replace the operator
  (`class Object; def +(o); end` leaves `1 + "s"` raising; `class
  Integer; def +` and `Integer.prepend(P)` do not), while the ARGUMENT's
  conversion hook is found anywhere in its ancestry (`Object#coerce`,
  `Kernel#coerce`, `String#coerce` each make `1 + "s"` print `2`). The
  hook is per operator — `to_str` for `String#+`, `coerce` for the
  numeric operators, and `to_int` is NOT consulted by either. A module
  `include`d into a core class never wins over the class's own method;
  only `prepend` does.
- A mechanism no mutant can distinguish is not a safeguard (learned
  2026-09-18, binding, measured): round 6 shipped a second reader for
  core-class bodies alongside the merged-fragment read, on the theory
  that reading the AST catches shapes the index misses. The mutation
  matrix disagreed — removing it left every test green — and the reason
  was structural: the walker attributes the same statements to the same
  fragment, so both readers always agreed. It was deleted, and the one
  shape the AST reader could NOT see (a reopening through a constant
  alias, `I = Integer; class I; def +`) is what the fragment read buys.
- A mutation harness that runs more than one test binary MUST pass
  `--no-fail-fast` (learned 2026-09-18, binding, measured): cargo stops
  after the first failing binary, so a mutant whose control lives in the
  second suite reported "expected X to fail" while X had never run. The
  harness was printing a real failure for the wrong reason, which is one
  step from printing a pass for the wrong reason.
- A mutation harness that rewrites the source IN PLACE makes every
  concurrent reader read a mutant (learned 2026-09-19, binding, measured):
  while `scripts/mixin-attribution-mutants.sh` held this checkout, reading
  `index.rs` showed the `method_missing` gate ABSENT — that was MUT-1a's own
  replacement text (`let _ = index.class(mid);`), and the lead nearly filed
  "the gate does not exist" about code that ships. `target/release/ita` is a
  mutant binary for part of the same window, so a concurrent measurement
  measures the mutant too. Read `git show HEAD:<path>` (or wait for the run),
  and never commit while a mutation harness holds the tree. A KILLED harness
  leaves its mutant in the tree (measured 2026-09-19: a gauntlet stopped by a
  3600 s job deadline left `crates/itaruby_semantic/src/index.rs` carrying a
  mutant, found by `git status` and restored from HEAD before anything else
  ran) — after any interrupted run, check the tree and restore; never resume
  on top of it, and never read a file the killed run was rewriting.
- A suppression that names one TRACK must not soften the other, and a
  mechanism no fixture or mutant can distinguish does not ship (learned
  2026-09-19, binding, measured): `X.include(M)` with `M` answering every
  name opens X for INSTANCE lookups, while the index's `open` flag is read by
  both lookups — so the attributed-mixin family attributes `include`/`prepend`
  only, and drops the `extend` arm whose only effect was silencing instance
  lookups `extend` never justifies (the three public corpora are byte-equal
  with it gone; `singleton_track_stays_closed` plus MUT-1c hold it down). The
  mirror direction — an instance-track openness softening a class-level call —
  is left exactly as it was, because no verdict can see it: this checker emits
  no singleton `NotFound` for a project class at all (probed 2026-09-19:
  `class X; end; X.absent_name` reports nothing), so a per-track reason would
  be machinery nothing could accuse. When a measurement cannot tell two
  behaviours apart, the honest output is that measurement, never the
  mechanism built on the guess.

## Closeout

1. Re-check changed paths against the DOX chain.
2. Update nearest owning docs and any affected parents or children.
3. Refresh every affected Child DOX Index.
4. Remove stale or contradictory text.
5. Run verification when relevant.
6. Report any docs intentionally left unchanged and why.

## Child DOX Index

- `docs/engineering-history.md` — the per-bead engineering log (waves 1-10,
  sections A-J) ported from the archived alpha repo; narrative and measured
  numbers, no contracts of its own.
- `scripts/public-baseline/README.md` — audit ledger of the public-corpus error sets.
- `scripts/bug-replay/README.md` — bug/fix replay benchmark — the "before production" recall metric; classifier HIT/MISS/NOISE/UNRELATED; `results/` is a run artifact.
- `docs/ruby-lsp-addon.md` — install + best practices for the ruby-lsp
  add-on; validated end-to-end 2026-09-03.

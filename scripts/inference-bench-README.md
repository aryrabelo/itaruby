# The inference bench

A small executable benchmark for one question: **does itaruby find real Ruby
bugs with zero annotations, and does it stay silent on dynamic code that is
correct at runtime?**

```
./scripts/inference-gate.sh          # build + judge, exit 0/1/2
ruby scripts/inference-bench.rb --sorbet   # judge only, comparison leg on
./scripts/inference-bench-selftest.sh      # prove the bench itself can see
```

## Ground truth is MRI, not an opinion and not the other checker

Every `accuse` case is executed and must really raise, at the line it is
blamed on. Every `silent` case is executed and must really exit 0. The bench
checks that *before* it judges any checker, so a fixture that does not do
what its row claims is reported as a broken fixture rather than scored as a
finding. This is what keeps "itaruby found a bug" from meaning "itaruby
printed something".

The alignment is exact today: for all six `accuse` cases the line MRI blames
is the line the checkers blame.

## Scope — read this before quoting any number

This is **not a parity benchmark**, and nothing here measures either tool on
a real application.

* Every fixture is self-contained, stdlib-only, **zero gems**. Neither
  declaration world is exercised: itaruby's curated `declarations/` and
  Sorbet's Tapioca/RBI gem pipeline are both out of play by construction.
* Sorbet runs at the sigil each fixture pins (`# typed: true`, first line of
  every fixture) under the `srb` version pinned in the manifest
  (`0.6.13437`). A different version is a **skip**, never a silent
  re-measurement against a different tool.
* When Sorbet reports on code that runs clean, the only claim made is
  **"Sorbet cannot prove this without an annotation"** — never "Sorbet cannot
  do this", never "Sorbet does not resolve this". That narrower sentence is
  earned by the `with_annotations` leg, never asserted: the leg re-runs
  Sorbet on the **same program** (byte-identical, sha-checked) plus the `sig`
  or RBI a human or Tapioca would write, and measures it clean. A row with no
  such leg is reported, never claimed.
* **Both directions are published.** Cases where Sorbet proves a bug that
  itaruby misses are first-class rows, listed below and in the scoreboard.
* **No timing claim.** This bench measures verdicts only. It makes and
  supports **no** statement about speed, wall time, or throughput for either
  tool; the speed bar lives in `scripts/perf-gate.sh` and the public-corpus
  gate, not here.

Any sentence quoted out of this bench must carry the full scope, which the
bench itself prints under `--- claim scope ---` with every number derived
from the manifest (it fails if rows disagree about the sigil or the pinned
version, since one sentence cannot then cover them):

> Measured on **12** self-contained fixtures, every one pinning
> **`# typed: true`**, against **srb 0.6.13437**, with **zero gems, zero
> Tapioca/generated RBIs, and zero itaruby curated declarations**. On the
> **3** rows carrying a fairness leg, *Sorbet cannot prove this without an
> annotation*. **No timing claim is made.** Not a parity benchmark.

## Isolation

Each case owns a directory and each tool is pointed at that directory alone:

```
testdata/gauntlet_inference/<case>/
  no_annotations/plain.rb     # both tools measured here, zero annotations
  with_annotations/plain.rb   # byte-identical copy (sha-checked by the bench)
  with_annotations/shim.rbi   # the annotation Sorbet needs, when it needs one
```

A shared `--dir` mixes results across cases, so the bench verifies that every
diagnostic's **path** belongs to the case being judged; a row reading on
another case's file fails.

## The ledger (12 cases, measured 2026-09-17; two rows re-pinned 2026-09-21)

| Case | Family | itaruby | Sorbet (no annotations) |
|---|---|---|---|
| `open_class_reopen_typo` | open class reopened | hit | hit |
| `namespace_reopen_typo` | nested + compact `class A::B` | hit | hit |
| `attr_reader_typo` | method-defining macro | hit | hit |
| `include_mixin_arity` | `include`, arity | hit | hit |
| `extend_singleton_typo` | `extend` → singleton | **hit (closed 2026-09-21)** | hit, plus one FP of its own |
| `included_hook_class_method_typo` | `self.included` + `base.extend` | **hit (closed 2026-09-21)** | hit |
| `define_method_loop` | `define_method` over a list | no false positive; **blind** (see below) | needs an annotation |
| `method_missing_proxy` | `method_missing` forwarder | correctly silent; **blind** (see below) | needs an annotation |
| `singleton_class_eval` | singleton `class_eval` | no false positive; **blind** (see below) | needs an annotation |
| `delegate_macro` | Rails-style `delegate` macro | silent (correct) | silent — control, no win either way |
| `respond_to_guard` | `respond_to?` capability guard | silent (correct) | silent — control |
| `const_missing_namespace` | `self.const_missing` | silent — **was an E0104 false positive, fixed 2026-09-17** | also reports |

Summary: 6 both prove, **0 Sorbet proves and itaruby does not**, 3 rows
where Sorbet needs an annotation and itaruby does not false-positive, 2
mutual controls, 2 itaruby false positives found by this bench and since
fixed (`const_missing_namespace` 2026-09-17, `singleton_class_eval`
2026-09-21 — see the Open debt section).

### Silence is not inference — the positive controls

The three `needs an annotation` rows each carry a `positive_control/`: the
same dynamic shape with a certain typo planted (`config.hostt`,
`Report.generatte`, `.deployy`). MRI raises on all three. **itaruby is silent
on all three too.**

So those rows say exactly one thing: itaruby does not false-positive on
correct dynamic code. They do **not** say it models the dynamic surface —
the class or the receiver is simply open, so nothing is provable either way,
and the silence is fail-closed. Read them as "no false positive here", never
as "understands this pattern". The bench prints this limitation itself.

### Open debt, recorded rather than ratcheted

* **The two real gaps CLOSED 2026-09-21** (`extend_singleton_typo`,
  `included_hook_class_method_typo`). The first attempt, on 2026-09-17,
  was built, measured and **reverted**: reporting the singleton
  `NotFound` residue added 703/36/4109 diagnostics to
  rails/mastodon/discourse, and even narrowed to explicit receivers plus
  a did-you-mean near miss it still added 216/0/12, all false
  (`SecureRandom.uuid`, `Kernel.rand`, `mattr_accessor` writers,
  `class << self` accessors). What shipped instead was a MEASUREMENT: the
  dark-singleton census bucketed every would-be accusation as
  `closed_notfound`, twelve beads turned each populated mechanism into a
  NAMED open reason or an indexed surface, the public-corpus residue fell
  from 54 records to 8, every survivor was read at its byte offset and
  proven to raise, and only then did the arm start emitting
  (`scripts/public-baseline/README.md`, AGENTS.md's class-object section).
  Both rows re-pinned here the same day; the mechanisms' mutants live in
  `scripts/class-object-flip-mutants.sh`.
* **This bench found the flip's one false positive, the day it armed.**
  `singleton_class_eval`'s no-annotation fixture
  (`Report.singleton_class.class_eval do define_method(:generate) ... end`
  then `Report.generate`) runs clean under MRI and the armed track
  accused it: `class << X` and `X.singleton_class.prepend M` were already
  held-aside singleton patches, but the eval/send family on the same
  receiver was not, so `Report` read as a complete closed class object.
  Fixed in `index.rs` the same day (bead ita-sce, fail-closed: the patch
  carries openness only, and an owner the project never declares is
  dropped). The row is a positive-control row, which is exactly why it
  caught this.
* `extend_singleton_typo` also shows Sorbet reporting `new` inside the
  extended module, a line that runs fine once the module is extended into a
  class. Recorded as observed; no fairness leg is filed for it, so the bench
  makes no claim about it.

## Why the bench has its own selftest

`scripts/inference-bench-selftest.sh` re-injects each way the bench could lie
and requires it to accuse the right case for the right reason: a bug silently
fixed (M1), a "clean" fixture that raises (M2), a fairness leg that drifts
from the program it is supposed to mirror (M3), a changed sigil (M4), a wrong
expected line (M5), a fixture nobody judges (M6), a fairness claim whose
annotation was deleted (M7), a version pin that no longer matches (M8), a
diagnostic leaking in from another file in the case directory (M9), a
positive control that stops raising and so controls for nothing (M10), an
undeclared positive control (M11), a positive-control verdict that drifts
from what was measured (M12), and a crashing binary whose empty output would
otherwise score as perfect silence (M13).

Three guards keep the harness from lying in turn: the unmutated lab must pass
first (else every later "caught" is noise), each mutant must name the case it
was injected into, and each mutant must leave every other case green — a
mutant that reddens the board proves nothing. The lab is built under
`target/` inside the repo on purpose: the bench derives its root from its own
location, so a lab outside the tree would silently judge the real tree.

## Re-measuring

The manifest `scripts/inference-bench.jsonl` is the expected-behavior ledger,
one JSON row per case. To re-pin after a deliberate change, run the bench,
read every divergence, and edit the row **only** once you have read the code
and can say why the new behavior is right — a diverging row is a finding
about the change, not about the manifest.

## Language

This repository is shared in English. New versioned text here — fixture
headers, expected-behavior comments, script output, this README, and every
benchmark label and limitation — uses **US spelling** (`behavior`,
`normalize`, `analyze`). Existing public API names and code conventions are
never renamed to match.

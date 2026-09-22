#!/usr/bin/env bash
# The gauntlet's gates, as exit codes instead of model calls. Every builder
# iteration runs this for free; the expensive critic only reads its artifacts.
#
#   0  every applicable gate green
#   1  a gate failed
#   2  gates green, but at least one declared corpus (scripts/corpus-baseline.txt)
#      was not found on this machine — the summary names which; full proof
#      needs every machine that holds a corpus (see AGENTS.md's Gates table)
#
# Corpus paths are machine-local and never versioned: scripts/corpora-local.txt
# (gitignored) maps each generic id from scripts/corpus-baseline.txt to a real
# path on this machine, one `<id> <path>` line per corpus — `<path>` is either
# absolute, or relative to $CORPORA_ROOT (default $HOME/Sites; override it if
# the whole corpora tree moves without any corpus being renamed). A missing
# scripts/corpora-local.txt, or a missing line for a declared id, skips that
# corpus exactly like a missing directory always has (exit 2, above).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
ITA=$ROOT/target/release/ita
CORPORA_ROOT=${CORPORA_ROOT:-$HOME/Sites}
BASELINE=$ROOT/scripts/corpus-baseline.txt
CORPORA_MAP=$ROOT/scripts/corpora-local.txt
ART=${ART:-$ROOT/target/gauntlet}

# Pin the cargo target dir to THIS tree unless the caller already chose
# one. Two worktrees of this repo share one `build.target-dir` on this
# machine (AGENTS.md, binding, measured): with it shared, `cargo test`
# here resolves the OTHER tree's `rmeta` and reports a red on code this
# tree does not contain, and two identically named tests race for the
# same `CARGO_TARGET_TMPDIR` subdirectory. This gate's own binary path
# (`$ITA` = `$ROOT/target/release/ita`) already assumes this tree's
# `target/`, so the two were inconsistent: the build wrote wherever the
# shared config said, and every binary-consuming gate then read a path
# that build never wrote.
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
mkdir -p "$ART"

# The transcript is written, not just printed. Everything below prints to
# stdout for the human AND appends to $ART/transcript.txt, because
# scripts/gate-digest reads the verdicts from there: a digest that had to
# re-derive each verdict from the artifacts would be a second opinion, not a
# summary of THIS run.
TRANSCRIPT=${TRANSCRIPT:-$ART/transcript.txt}
: >"$TRANSCRIPT"

# The load at gate start, captured once, because a timing gate read without it
# is a number with no conditions attached (this machine routinely runs several
# agent sessions while the gates run).
uptime >"$ART/load.txt" 2>/dev/null || : >"$ART/load.txt"

failed=0
skipped=0
log()  { printf '%s\n' "$1" >>"$TRANSCRIPT"; }
say()  { printf '\n=== %s\n' "$1"; log "=== $1"; }
ok()   { printf 'PASS %s\n' "$1"; log "PASS $1"; }
bad()  { printf 'FAIL %s\n' "$1"; failed=1; log "FAIL $1"; }
skip() { printf 'SKIP %s — %s\n' "$1" "$2"; skipped=1; log "SKIP $1 — $2"; }
out()  { printf '%s\n' "$1"; log "$1"; }

# One compact JSON verdict at $ART/digest.json, always fresh, on every exit
# path — including the early ones, which are exactly the runs a reader most
# needs explained. It is a REPORTER: its own failure can never change this
# script's exit code (AGENTS.md: a gate's verdict stays an exit code).
digest() {
  "$ROOT/scripts/gate-digest" --art "$ART" --transcript "$TRANSCRIPT" \
    --out "$ART/digest.json" >/dev/null 2>"$ART/digest-stderr.txt" || true
}

# --- gate 0: software-factory. Tamper-proof by design; a rule that blocks you
# is a finding about your change, never about the rule.
#
# Two runs, not one. Plain `sf check` judges the tree as it stands. The
# `--changed` run is what gives the direction-reading L2 rules a base to
# compare against — without it POLICY_ONLY_TIGHTENS reports green while
# being inert, which is how a loosening walks past a local gate (measured
# 2026-08-26: disabling a rule in policy passed plain `sf check`).
say 'gate 0 — sf check'
if ! command -v sf >/dev/null; then
  bad 'sf check (sf not installed — cargo install --git https://github.com/nicolasmelo1/software-factory --locked)'
else
  if sf check >"$ART/sf.txt" 2>&1; then ok 'sf check'; else bad 'sf check (see target/gauntlet/sf.txt)'; fi
  if sf check --changed HEAD >"$ART/sf-changed.txt" 2>&1; then
    ok 'sf check --changed HEAD (the guardrail only tightened)'
  else
    bad 'sf check --changed HEAD (see target/gauntlet/sf-changed.txt)'
  fi
fi

# --- gate 0b: the MRI interpreter the operand-type fixtures run under.
# Those fixtures use endless method definitions (`def m(x) = …`), so a `ruby`
# older than 3.0 makes gate a fail INSIDE a fixture with a syntax error — a red
# for the wrong reason, and one that is only understood after the whole
# mutation pass has burned (measured 2026-09-19: ~70 minutes of gate c1 spent
# on /usr/bin/ruby 2.6 while a 3.4.2 sat first on the interactive PATH, and
# both mutant families that run the operand-types suite then aborted with
# "baseline is not green"). A MISSING ruby is not a failure — the MRI legs
# skip by design — but an old one is, and saying so here costs two seconds.
say 'gate 0b — MRI interpreter precondition'
if ruby_bin=$(command -v ruby 2>/dev/null); then
  ruby_ver=$(ruby -v 2>&1 | sed -n '1p')
  ruby_major=$(printf '%s' "$ruby_ver" | sed -n 's/^ruby \([0-9][0-9]*\)\..*/\1/p')
  if [[ -n $ruby_major && $ruby_major -ge 3 ]]; then
    ok "ruby $ruby_major.x ($ruby_bin)"
  else
    bad "ruby too old for the operand-type fixtures (need 3.x+: they use endless defs): '$ruby_ver' at $ruby_bin — put a 3.x ruby first on PATH"
  fi
else
  ok 'no ruby on PATH (the MRI legs skip by design)'
fi

say 'gate a — cargo test --workspace'
if cargo test --workspace >"$ART/tests.txt" 2>&1; then ok 'cargo test'; else bad 'cargo test (see target/gauntlet/tests.txt)'; fi

say 'gate b — build release binary'
if cargo build --release --locked --target-dir "$ROOT/target" >"$ART/build.txt" 2>&1; then
  ok 'cargo build --release'
else
  bad 'cargo build --release (see target/gauntlet/build.txt)'
  out 'RESULT: FAIL (build failed; no binary-consuming gates executed)'
  digest
  exit 1
fi

# --- gate c: mutation probe. Both directions, because a checker that is silent
# on correct code AND on broken code is blind, not safe.
say 'gate c — mutation probe (testdata/)'
if [[ -x $ITA ]]; then
  before=$failed
  "$ITA" check testdata/ >"$ART/testdata.txt" 2>&1
  [[ $? -eq 1 ]] || bad 'mutation probe: ita check testdata/ must exit 1'
  # ita-yho: E0106 is a Warning, never an Error, so its line reads
  # "warning[E0106]" while the other planted codes are all errors —
  # matching either severity keeps this one array the single source of
  # truth for "which codes must still be caught" per AGENTS.md's
  # instruction to touch only this array.
  #
  # E0108 is NOT in that array, and the reason is a decision working as
  # designed rather than a hole: `testdata/included_hook/`
  # `dynamic_hook_still_warns.rb` contains `base.class_eval(string)` — an
  # eval body on a receiver nobody can name — which stands EVERY core
  # class down project-wide (`FileScan::note_opaque_eval`), and
  # `ita check testdata/` is ONE merged project. Measured: with that one
  # file moved aside the merged run reports 5 E0108 again, with it in
  # place, 0. So the E0108 half of this probe runs against the directory
  # whose fixtures are about E0108, where the proof still means
  # something; the same five fixtures, the same requirement. A poisoning
  # fixture landing INSIDE that directory fails this line loudly instead
  # of being absorbed by a merged run.
  for code in E0001 E0101 E0102 E0103 E0106 E0107; do
    grep -qE "(error|warning)\[$code\]" "$ART/testdata.txt" || bad "mutation probe: planted $code no longer caught"
  done
  "$ITA" check testdata/operand_types >"$ART/testdata-operand-types.txt" 2>&1
  grep -qE 'error\[E0108\]' "$ART/testdata-operand-types.txt" \
    || bad 'mutation probe: planted E0108 no longer caught in testdata/operand_types'
  # invariant #1, silence side: these two are correct code and must stay quiet.
  for quiet in open_class.rb reopen.rb; do
    if grep -q "testdata/$quiet" "$ART/testdata.txt"; then
      bad "invariant #1: false positive in testdata/$quiet"
    fi
  done
  (( failed == before )) && ok 'mutation probe (planted bugs caught, correct code silent)'
else
  bad 'mutation probe: no target/release/ita'
fi

# --- gate c1: per-fix source mutants. Gate c above proves the CODES still
# fire; these prove each load-bearing DECISION inside a fix is load-bearing,
# by removing exactly one at a time and demanding a NAMED test fail. Both
# scripts existed before this wiring and neither ran in any gate — a probe
# nothing executes decays into narration (AGENTS.md: the probes proving each
# side live next to the instrument). The `mixin-attribution` family (bead
# ita-a8z) joined the same loop when it was written, for the same reason:
# it is the only thing that would notice one of the three suppression
# mechanisms being cut. They edit source in place and restore it
# byte-identical with `cmp`, so they run after gate b's build and before any
# other binary-consuming gate rebuilds.
# Fase A/onda 2 added five more families on the same terms: each defends one
# fail-closed decision that nothing else would notice being cut — the
# lazy-load base openness (bead B), the self.extended hook harvest (bead H),
# the two predicate-guard shapes (bead C), the asserted-raise subject span
# (bead E) and the rebindable-block guard moving above the lookup dispatch
# (bead F). They are listed here or they are narration.
say 'gate c1 — per-fix source mutants (each decision accused by a named test)'
# Every anchor must match exactly once BEFORE the families run. The harness's
# own guard fires 20-30 minutes in, after the mutants ahead of it have already
# run, and what it reports is a red for a reason that has nothing to do with
# the checker (measured 2026-09-19: wave 2's neighbour insertions killed M14 and
# M15 in the operand-types family, and the failure surfaced as
# `baseline is not green` on the two families that run that suite).
if "$ROOT/scripts/mutant-anchors" >"$ART/anchors.txt" 2>&1; then
  ok 'mutant anchors (every needle matches exactly once)'
else
  bad 'mutant anchors (see target/gauntlet/anchors.txt)'
fi
for m in const-missing operand-types singleton mixin-attribution \
         lazy-load extended-hook guard-narrowing asserted-raise rebindable-guard; do
  if "$ROOT/scripts/$m-mutants.sh" >"$ART/$m-mutants.txt" 2>&1; then
    ok "$m mutants (every mutant accused, source restored byte-identical)"
  else
    bad "$m mutants (see target/gauntlet/$m-mutants.txt)"
  fi
done
# This family mutates an isolated public-source copy, not the checkout.
if python3 "$ROOT/scripts/conflicting-superclasses-mutants.py" >"$ART/conflicting-superclasses-mutants.txt" 2>&1; then
  ok 'conflicting-superclasses mutants (each decision accused, isolated source copy)'
else
  bad 'conflicting-superclasses mutants (see target/gauntlet/conflicting-superclasses-mutants.txt)'
fi
# Those scripts leave $ROOT/target holding a binary built from the LAST
# mutant they injected only if they died mid-run; on success they rebuild
# the shipped source. Rebuild anyway: the gates below measure this binary,
# and a stale one is the 2026-09-17 defect (AGENTS.md).
if ! cargo build --release --locked --target-dir "$ROOT/target" >>"$ART/build.txt" 2>&1; then
  bad 'rebuild after the source mutants failed'
  out 'RESULT: FAIL (no trustworthy binary for the remaining gates)'
  digest
  exit 1
fi

# --- gate c2: unwrap shape. Answers "are any of these 60-odd unwraps a real
# panic path?" as a checked property instead of a reading exercise that decays
# with the next commit. See scripts/unwrap-gate.sh's header.
say 'gate c2 — unwrap shape (production source)'
if "$ROOT/scripts/unwrap-gate.sh" >"$ART/unwrap.txt" 2>&1; then
  ok 'unwrap shape (only prism downcasts, each guarded by its match arm)'
else
  bad 'unwrap shape (see target/gauntlet/unwrap.txt)'
fi

# --- gate c2b: the evidence instruments themselves. Every other gate here
# trusts two scripts to PRODUCE honest evidence; on 2026-09-17 all three of
# these defects were live and invisible: this script ran binary-consuming
# gates on a stale binary after a failed build, and replay.sh both appended
# every run into one file (a previous day's MISS reading as today's result)
# and built into cargo's global target dir while measuring $ROOT/target's
# binary. A gate whose evidence producer can lie is not a gate, so the
# producers get the same two-sided treatment as the checker: each defect is
# re-injected as a mutant and must reproduce, and the shipped script must
# stay clean. See scripts/instrument-mutants.sh's header.
say 'gate c2b — evidence instruments (two-sided)'
if INSTRUMENT_MUTANTS_LAB="$ART/instrument-mutants" \
   "$ROOT/scripts/instrument-mutants.sh" >"$ART/instrument-mutants.txt" 2>&1; then
  ok 'evidence instruments (each defect reproduces under mutation, shipped scripts clean)'
else
  bad 'evidence instruments (see target/gauntlet/instrument-mutants.txt)'
fi
# Same family, one level up: scripts/gate-digest is what a reader (human or
# model) now believes INSTEAD of the 115 MB of artifacts, so it is itself an
# evidence producer and gets the same two-sided proof — four fixture
# gauntlet directories it must report exactly, and five cmp-guarded mutants
# each of which must be accused by its own named case.
if GATE_DIGEST_LAB="$ART/gate-digest-selftest" \
   "$ROOT/scripts/gate-digest-selftest.sh" >"$ART/gate-digest-selftest.txt" 2>&1; then
  ok 'gate digest (each fixture reported exactly, every mutant accused by its case)'
else
  bad 'gate digest (see target/gauntlet/gate-digest-selftest.txt)'
fi
# Same family, one level further: scripts/gate-triage reads the digest a
# reader trusts and routes the repair. A router that fires on green or
# misses a known failure shape costs the lead a wasted repair, so it gets
# the same two-sided proof — eight fixture runs routed exactly (green routes
# NOTHING), ten cmp-guarded mutants each accused by its named guard, and the
# state it sends to the model proved a pure function of digest.json (the
# secrecy wall travels with the digest). Offline by construction.
if GATE_TRIAGE_LAB="$ART/gate-triage-selftest" \
   "$ROOT/scripts/gate-triage-selftest.sh" >"$ART/gate-triage-selftest.txt" 2>&1; then
  ok 'gate triage (each fixture routed exactly, every mutant accused by its guard)'
else
  bad 'gate triage (see target/gauntlet/gate-triage-selftest.txt)'
fi
# Same family again, now on the public-corpus side. `public-gate.sh` compares a
# clone against a baseline, and since 2026-09-19 it also verifies the clone's
# HEAD against the declared sha — a guard that can be inert (the wave-9 trap:
# `PASS (incomplete)` while judging another revision) is exactly the defect
# this selftest re-injects, in both halves (missing guard, wrong-rev guard).
if PUBLIC_GATE_LAB="$ART/public-gate-selftest" \
   "$ROOT/scripts/public-gate-selftest.sh" >"$ART/public-gate-selftest.txt" 2>&1; then
  ok 'public gate guard (match / wrong-rev / missing / default-skip, each mutant accused)'
else
  bad 'public gate guard (see target/gauntlet/public-gate-selftest.txt)'
fi
# And the reader of that diff: `public-drift-attrib` decides whether a new
# line is a real detection change or the same diagnostic that moved, which is
# the difference between "the checker regressed" and "the corpus advanced".
# A classifier that reports everything as drift, or drops an unparseable line
# silently, makes gate e's verdict meaningless — so it gets the same
# two-sided proof (7 fixtures + a comm oracle + a malformed hard-stop).
if PUBLIC_DRIFT_LAB="$ART/public-drift-attrib-selftest" \
   "$ROOT/scripts/public-drift-attrib-selftest.sh" >"$ART/public-drift-attrib-selftest.txt" 2>&1; then
  ok 'drift attribution (line-shift vs real drift, malformed hard-stop, each mutant accused)'
else
  bad 'drift attribution (see target/gauntlet/public-drift-attrib-selftest.txt)'
fi

# --- gate c3: performance ceiling. The launch bar for this project is speed
# against an established typechecker, and nothing used to notice a regression.
# Slack is debt: when this reports comfortably under its ceiling, tighten the
# ceiling in the same commit (AGENTS.md, binding).
say 'gate c3 — performance ceiling'
if "$ROOT/scripts/perf-gate.sh" >"$ART/perf.txt" 2>&1; then
  ok "performance ceiling ($(grep -c ' ok$' "$ART/perf.txt" || true) bench(es) under ceiling; see target/gauntlet/perf.txt)"
else
  bad 'performance ceiling (see target/gauntlet/perf.txt)'
fi

# --- gate d: corpus diff. The baselines are sacred; identity is proved by the
# SHA-256 hash of each normalized error line, never by the line's own text —
# see scripts/corpus-baseline.txt's header for why (client paths never land
# in a versioned file, hence the generic ids and the gitignored, per-machine
# scripts/corpora-local.txt that maps them to real paths). Each declared
# corpus is checked only where its id has a mapped, existing path on this
# machine; unmapped or absent corpora are skipped individually, by id — full
# proof needs both m5 and work, see AGENTS.md's Gates section.
say 'gate d — corpus diff'
corpus_proved=()
corpus_skipped=()
while read -r _ id ceiling_kv; do
  ceiling=${ceiling_kv#warn_ceiling=}
  path=""
  if [[ -f $CORPORA_MAP ]]; then
    path=$(awk -v id="$id" '$1 == id { print $2; exit }' "$CORPORA_MAP")
  fi
  if [[ -z $path ]]; then
    corpus_skipped+=("$id")
    skip "corpus diff ($id)" "no path for $id in scripts/corpora-local.txt"
    continue
  fi
  case $path in
    /*) root=$path ;;
    *)  root="$CORPORA_ROOT/$path" ;;
  esac
  if [[ ! -d $root ]]; then
    corpus_skipped+=("$id")
    skip "corpus diff ($id)" "not found at $root"
    continue
  fi
  if [[ ! -x $ITA ]]; then
    bad "corpus diff ($id): no target/release/ita"
    continue
  fi
  out="$ART/corpus-$id.txt"
  "$ITA" check "$root" >"$out" 2>&1
  # WHICH revision produced this verdict, and which one the baseline pinned.
  # Without both in the transcript a red gate d cannot be attributed: identical
  # detection over a corpus that moved reads exactly like a regression
  # (learned 2026-09-18, binding, measured — see AGENTS.md's Rules of proof).
  # A bare git sha identifies no client, so it is safe to print and to pin.
  rev=$(git -C "$root" rev-parse --short HEAD 2>/dev/null || true)
  rev=${rev:-unknown}
  dirty=$(git -C "$root" status --porcelain 2>/dev/null | wc -l | tr -d ' ')
  (( dirty > 0 )) && rev="$rev+$dirty"
  pinned=$(awk -v id="$id" '/^# corpus_rev /{ r = $3 } $1 == "corpus" && $2 == id { print r; exit }' "$BASELINE")
  at="[rev $rev, baseline pinned ${pinned:-none}]"
  # Normalization: strip this corpus's mapped path prefix and replace it with
  # `<id>/`, so the hash depends on neither the machine's mount point nor the
  # corpus's own directory name/nesting — only on the id and the file's path
  # relative to the corpus root.
  sed "s|^$root/|$id/|" "$out" | grep 'error\[' | sort -u >"$ART/corpus-$id-errors.txt"
  : >"$ART/corpus-$id-error-hashes.txt"
  while IFS= read -r line; do
    [[ -z $line ]] && continue
    printf '%s\n' "$line" | shasum -a 256 | cut -c1-16
  done <"$ART/corpus-$id-errors.txt" | sort -u >"$ART/corpus-$id-error-hashes.txt"
  grep "^err $id " "$BASELINE" | awk '{print $3}' | sort -u >"$ART/corpus-$id-expected-hashes.txt"
  comm -23 "$ART/corpus-$id-error-hashes.txt" "$ART/corpus-$id-expected-hashes.txt" >"$ART/corpus-$id-new-hashes.txt"
  comm -13 "$ART/corpus-$id-error-hashes.txt" "$ART/corpus-$id-expected-hashes.txt" >"$ART/corpus-$id-gone-hashes.txt"
  new_count=$(wc -l <"$ART/corpus-$id-new-hashes.txt" | tr -d ' ')
  gone_count=$(wc -l <"$ART/corpus-$id-gone-hashes.txt" | tr -d ' ')
  if (( new_count == 0 && gone_count == 0 )); then
    ok "corpus errors match baseline exactly ($id) $at"
  else
    drift="$ART/corpus-$id-drift.txt"
    {
      echo "# drift for corpus $id — in the clear, stays on this machine (gitignored)"
      echo '## new (present now, not in baseline) — needs a true-positive audit before it joins the baseline:'
      while IFS= read -r line; do
        [[ -z $line ]] && continue
        h=$(printf '%s\n' "$line" | shasum -a 256 | cut -c1-16)
        grep -qxF "$h" "$ART/corpus-$id-new-hashes.txt" && echo "$line"
      done <"$ART/corpus-$id-errors.txt"
      echo '## missing (baseline expects these hashes, none produced now — a regression, text unavailable since only the hash was ever stored):'
      cat "$ART/corpus-$id-gone-hashes.txt"
    } >"$drift"
    bad "corpus errors drifted ($id): $new_count new, $gone_count missing $at (see target/gauntlet/corpus-$id-drift.txt)"
  fi
  got_warn=$(grep -c 'warning\[' "$out")
  if (( got_warn > ceiling )); then
    bad "warning ceiling ($id): $got_warn > $ceiling $at"
  else
    ok "warning ceiling ($id): $got_warn <= $ceiling $at"
  fi
  corpus_proved+=("$id")
done < <(grep '^corpus ' "$BASELINE")

# --- gate e: navigation oracle. Ground truth is a real Ruby app's own
# Object#method(:x).source_location, harvested by scripts/nav-oracle.rb;
# scripts/nav-gate.sh compares itaruby's `ita definition` against it and
# judges. Only real app_def pairs can fail — invariant #1 extended to
# navigation: no answer is fine, a wrong one never is (see AGENTS.md).
# scripts/navfixture/oracle.jsonl is a fixture the repo itself carries
# (harvested from scripts/navfixture/workload.rb, a stdlib-only toy app —
# see its header comment to regenerate it), so this gate judges on every
# machine, no bootable client app required; a missing/corrupt repo fixture
# is `bad`, not `skip` — it is checked into git, so its absence is a
# regression, not a machine limitation. NAV_ORACLE_JSONL still lets a
# pre-collected fixture from a real app add extra coverage: when set, the
# gate judges it too and only passes when both fixtures do.
# ponytail: no more auto-harvest-from-a-booted-Rails-app fallback here —
# that always needed the corpora machine anyway (this gate's whole reason
# for existing was that it *didn't*), and an already-harvested fixture path
# is exactly what NAV_ORACLE_JSONL means to scripts/nav-gate.sh already.
# Add a harvest step back if/when an opt-in real-app harvest is wanted.
say 'gate e — navigation oracle'
REPO_FIXTURE=$ROOT/scripts/navfixture/oracle.jsonl
if [[ ! -s $REPO_FIXTURE ]]; then
  bad 'navigation oracle: scripts/navfixture/oracle.jsonl missing or empty (versioned fixture — its absence is a regression)'
else
  ART="$ART/nav-repo" "$ROOT/scripts/nav-gate.sh" "$REPO_FIXTURE"
  case $? in
    0) ok 'navigation oracle: scripts/navfixture/oracle.jsonl (ita definition matches the runtime for every app_def pair it answered)' ;;
    1) bad 'navigation oracle: scripts/navfixture/oracle.jsonl (see target/gauntlet/nav-repo/nav-mismatches.txt)' ;;
    *) bad 'navigation oracle: scripts/navfixture/oracle.jsonl (nav-gate.sh errored)' ;;
  esac

  if [[ -n ${NAV_ORACLE_JSONL:-} && -s $NAV_ORACLE_JSONL ]]; then
    ART="$ART/nav-external" "$ROOT/scripts/nav-gate.sh" "$NAV_ORACLE_JSONL"
    case $? in
      0) ok "navigation oracle: external fixture ($NAV_ORACLE_JSONL)" ;;
      1) bad 'navigation oracle: external fixture (see target/gauntlet/nav-external/nav-mismatches.txt)' ;;
      *) bad 'navigation oracle: external fixture (nav-gate.sh errored)' ;;
    esac
  fi
fi

# --- gate f: public corpus. The launch bar (AGENTS.md) is beating Sorbet on
# speed and true errors on four PUBLIC Rails apps; that measurement is now an
# exit code. Runs scripts/public-gate.sh, which reads the repo declarations
# from scripts/public-corpora.txt, clones them only when PUBLIC_GATE_CLONE=1
# (never automatically — gitlab-foss alone is multi-GB, so the human
# decides when a machine may spend the disk), and compares each repo's
# SORTED diagnostic set against its in-clear baseline in
# scripts/public-baseline/<id>.jsonl, plus a per-repo wall-time ceiling.
# Baselines are versioned; a missing clone, a missing baseline, or a
# broken binary skips that repo individually (exit 2), never fails it.
say 'gate f — public corpus'
if "$ROOT/scripts/public-gate.sh" >"$ART/public.txt" 2>&1; then
  ok 'public corpus'
else
  case $? in
    1) bad 'public corpus (see target/gauntlet/public.txt)' ;;
    2) skip 'public corpus' 'at least one declared repo could not be judged on this machine (see target/gauntlet/public.txt)' ;;
    *) bad 'public corpus (see target/gauntlet/public.txt)' ;;
  esac
fi

# --- gate g: the inference bench. The launch bar is annotation-free
# inference that finds real bugs without inventing any, and until now
# nothing measured that as an exit code on code whose truth is known. Each
# case is executed under MRI first, so "real bug" means the program really
# raises and "correct code" means it really runs; the checker is judged
# against that, never against another checker's opinion. Sorbet is measured
# alongside where `srb` exists, at a pinned version, with a fairness leg
# that re-measures the SAME program once annotated — the only thing that
# licenses the narrow claim "Sorbet cannot prove this without an
# annotation". Both directions are published, our two misses included.
# The bench's own two-sided proof is scripts/inference-bench-selftest.sh;
# see scripts/inference-bench-README.md for scope and limits.
say 'gate g — inference bench (annotation-free, MRI as ground truth)'
if "$ROOT/scripts/inference-bench-selftest.sh" >"$ART/inference-selftest.txt" 2>&1; then
  if "$ROOT/scripts/inference-gate.sh" >"$ART/inference.txt" 2>&1; then
    ok 'inference bench'
  else
    case $? in
      2) skip 'inference bench' 'a leg was skipped on this machine (see target/gauntlet/inference.txt)' ;;
      *) bad 'inference bench (see target/gauntlet/inference.txt)' ;;
    esac
  fi
else
  case $? in
    2) skip 'inference bench' 'selftest could not run on this machine (see target/gauntlet/inference-selftest.txt)' ;;
    *) bad 'inference bench selftest — the bench cannot see its own defects (see target/gauntlet/inference-selftest.txt)' ;;
  esac
fi

say 'summary'
(( ${#corpus_proved[@]} )) && out "corpus gate proved: ${corpus_proved[*]}"
(( ${#corpus_skipped[@]} )) && out "corpus gate skipped (not found on this machine): ${corpus_skipped[*]}"
if (( failed )); then out 'RESULT: FAIL'; digest; exit 1; fi
if (( skipped )); then out "RESULT: PASS (incomplete — skipped: ${corpus_skipped[*]})"; digest; exit 2; fi
out 'RESULT: PASS'
digest

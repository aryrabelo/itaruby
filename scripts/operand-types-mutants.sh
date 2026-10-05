#!/usr/bin/env bash
# Two-sided proof for E0108 (operator operand type mismatch) and for the
# refinement suppression it depends on. AGENTS.md, binding: the probes
# proving each side live next to the instrument, never only narrated —
# this file is where the mutation matrix lives, instead of in a scratch
# harness nobody can grep for.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/operand_types.rs
# or crates/itaruby_semantic/tests/core_conclusive.rs — both suites run on
# every mutant, because round 5 split the question in two: E0108 reads the
# NAME-KEYED pollution maps (`core_ops_unpolluted`), while the blanket
# fields M13..M28 cover (`refined_core`, `eval_polluted_core`,
# `core_mixin`) are now read only by the closed-world core lookup, whose
# controls live in `core_conclusive.rs`.
# A mutant that compiles with the suite green is a missing control, not a
# safe change (AGENTS.md): add the fixture, then count the mutant.
#
# Label numbering is the one the rounds produced, kept so the commit
# messages and the CHANGELOG still name the same mutants:
#   M1a/M1b  round 3 — each half of the pollution gate
#   M2..M7   round 3 — each half of the proof, the `+`-only restriction,
#            the severity, the parameter poisoning, the local half
#   M13..M20 round 4 — the refinement collector and its decisions
#   M21..M28 round 5 — the eval-body collector and its decisions
#   M29..M47 round 6 — name-keyed pollution: the two asymmetric sides,
#            the per-pairing coercion hook, the readable/`Opaque` split
#            in a class body, the fragment half of the collector, and the
#            `Object` guards the corpora measured (M1a/M1b re-anchored
#            onto `core_ops_unpolluted`, which replaced E0108's two
#            blanket reads; M13/M17..M26's controls moved to
#            `core_conclusive.rs` with the fields they read)
# M8..M12 were round 3's refinement mutants, written against the arm in
# `DefWalker::walk_body`; round 4 replaced that arm with `FileScan`, so
# their anchors no longer exist and M13/M16..M20 supersede them one for
# one. Nothing is silently dropped: every decision they covered is still
# named below.
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise
# resolve each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CHK=crates/itaruby_semantic/src/check.rs
IDX=crates/itaruby_semantic/src/index.rs
CORE=crates/itaruby_semantic/src/core.rs
BAK_CHK=$ROOT/target/operand-types-check.rs.orig
BAK_IDX=$ROOT/target/operand-types-index.rs.orig
BAK_CORE=$ROOT/target/operand-types-core.rs.orig
LOG=$ROOT/target/operand-types-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$CHK" "$BAK_CHK"
cp "$IDX" "$BAK_IDX"
cp "$CORE" "$BAK_CORE"
restore() { cp "$BAK_CHK" "$CHK"; cp "$BAK_IDX" "$IDX"; cp "$BAK_CORE" "$CORE"; }
trap restore EXIT

# One test-binary run.
compiles=0
failing=
# Sets `compiles` and `failing` in the CALLER's shell — never called in a
# command substitution, because a subshell's assignments die with it and
# "did not compile" would read as "green".
run_suite() {
  # `--no-fail-fast` is load-bearing, not hygiene: cargo stops after the
  # first failing test BINARY, so without it a mutant whose control lives
  # in the second suite reported "expected X to fail" while X had never
  # run (caught by M15 in round 6).
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test operand_types --test core_conclusive >"$LOG" 2>&1
  if grep -q 'could not compile' "$LOG"; then compiles=0; else compiles=1; fi
  failing=$(grep -oE '^test [a-z0-9_]+ \.\.\. FAILED' "$LOG" | awk '{print $2}' | sort -u)
}

# `sed` cannot express these anchors: several span lines and all of them
# must match EXACTLY once. A miss prints INVALIDO and fails the script
# rather than silently mutating nothing — a mutant that was never
# injected is not a passing mutant.
inject() {
  NEEDLE=$2 REPL=$3 python3 - "$1" <<'PY'
import os, sys
path = sys.argv[1]
src = open(path).read()
needle, repl = os.environ['NEEDLE'], os.environ['REPL']
n = src.count(needle)
if n != 1:
    print(f'INVALIDO: anchor matched {n} times in {path}', file=sys.stderr)
    sys.exit(2)
open(path, 'w').write(src.replace(needle, repl))
PY
}

# mutant <label> <file> <needle> <replacement> <must-fail test> <rationale>
mutant() {
  local label=$1 file=$2 needle=$3 repl=$4 expect=$5 why=$6
  printf '\n--- %s: %s\n' "$label" "$why"
  restore
  if ! inject "$file" "$needle" "$repl"; then
    bad "$label INVALIDO — anchor not found, mutant never injected"
    restore
    return
  fi
  run_suite
  if (( ! compiles )); then
    bad "$label did not compile — a mutant that cannot run proves nothing"
  elif [[ -z $failing ]]; then
    bad "$label BLIND — compiles and the suite is green: the control is missing, not the change safe"
  elif grep -qx "$expect" <<<"$failing"; then
    ok "$label accused by $expect ($(tr '\n' ' ' <<<"$failing" | sed 's/ $//'))"
  else
    bad "$label expected $expect to fail, got: $(tr '\n' ' ' <<<"$failing")"
  fi
  restore
  cmp -s "$CHK" "$BAK_CHK" && cmp -s "$IDX" "$BAK_IDX" && cmp -s "$CORE" "$BAK_CORE" \
    || bad "$label source NOT restored byte-identical"
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green — failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (every accusing fixture fires, every silent fixture quiet)'

# ---- E0108 itself (round 3) -------------------------------------------

mutant M1a "$CHK" \
  '        self.names_unpolluted(core_own_names(recv_cc), &[op])
            && self.names_unpolluted(core_pollution_names(arg_cc), &arg_keys)' \
  '        self.names_unpolluted(core_pollution_names(arg_cc), &arg_keys)' \
  reopened_integer_with_custom_plus_is_silent \
  'the RECEIVER-side pollution gate: `class Integer; def +` prints joined:R$ under MRI'

mutant M1b "$CHK" \
  '        self.names_unpolluted(core_own_names(recv_cc), &[op])
            && self.names_unpolluted(core_pollution_names(arg_cc), &arg_keys)' \
  '        self.names_unpolluted(core_own_names(recv_cc), &[op])' \
  string_with_coerce_is_silent \
  'the ARGUMENT-side gate: `coerce` lives on the argument class, and that program prints 200'

mutant M2 "$CHK" \
  '        let proven = self.operand_locals.get(&name)?.clone();
        (env.get(&name) == Some(&proven)).then_some(proven)' \
  '        let proven = env.get(&name)?.clone();
        Some(proven)' \
  a_later_reassignment_silences_an_earlier_site \
  'the flow-INSENSITIVE half: the env alone cannot see a reassignment BELOW the operator'

mutant M3 "$CHK" \
  '        let proven = self.operand_locals.get(&name)?.clone();
        (env.get(&name) == Some(&proven)).then_some(proven)' \
  '        let proven = self.operand_locals.get(&name)?.clone();
        Some(proven)' \
  a_conditionally_assigned_operand_is_not_proven \
  'the flow-SENSITIVE half: one literal write does not prove the value on a skipped branch'

mutant M4 "$CHK" \
  '            (Ty::Str, Ty::Int | Ty::Nil) if op == b"+" => "a String operand",' \
  '            (Ty::Str, Ty::Int | Ty::Nil) => "a String operand",' \
  legal_pairings_stay_silent \
  'the `+`-only restriction on a String receiver: `"ab" * 2` is legal Ruby'

mutant M5 "$CHK" \
  '            E0108_OPERAND_TYPE_MISMATCH,
            Severity::Error,' \
  '            E0108_OPERAND_TYPE_MISMATCH,
            Severity::Warning,' \
  integer_plus_string_accuses_with_pinned_span_and_message \
  'the severity: the program really crashes on that line, so it is never a Warning'

mutant M6 "$CHK" \
  '        let proven = self.scope_operand_locals(
            def.parameters().map(|p| p.as_node()).as_ref(),
            def.body().as_ref(),
        );' \
  '        let proven = self.scope_operand_locals(None, def.body().as_ref());' \
  silent_fixtures_report_nothing \
  'parameter poisoning: a parameter value comes from the caller, never from the body literal'

mutant M7 "$CHK" \
  '        let local = node.as_local_variable_read_node()?;' \
  '        let local = node.as_local_variable_read_node().filter(|_| false)?;' \
  integer_plus_string_accuses_with_pinned_span_and_message \
  'the local half: without it only two literals in one expression could ever fire'

# ---- the refinement suppression (round 4) -----------------------------

mutant M13 "$IDX" \
  '        if let Some(expanded) = index.expand_unresolved_alias_target(&nesting, &name) {
            index.refined_core.insert(expanded.trim_start_matches("::").to_string());
        }' \
  '        let _ = &nesting;' \
  a_refinement_through_an_alias_stands_its_class_down \
  'alias resolution: `I = Integer; refine I` prints "aliased s" under MRI'

# The anchor carries the statement IMMEDIATELY above the recursion, because
# the bare recursion line stopped being unique on 2026-09-17: the
# singleton track added two nested `Visit` impls to this file whose own
# recursion lines contain this one as a substring (they sit one block
# deeper, so the 8-space needle matches inside their 12-space lines) —
# three matches, reported as INVALIDO rather than mutating a coin flip.
# `self.note_refinement(node);` was the first attempt and is NOT adjacent
# to the recursion: `note_opaque_eval` and `note_injection` sit between
# them, so that anchor matched zero times and the mutant was never
# injected (measured 2026-09-18, sha 63749fc: `FAIL M14 INVALIDO`).
#
# Rewritten again by the same cause (measured 2026-09-19, wave 2's
# load-hook family): `note_load_hook_base` joined the chain between
# `note_injection` and the recursion, and `def_locals` became part of the
# def frame — so BOTH anchors matched zero times and gate c1 reported
# `FAIL operand-types mutants` with M14 and M15 INVALIDO, 26 minutes into
# a family that had already run its other 45 mutants. The neighbour
# statement is the fragile half of every anchor here: re-count it with the
# harness's own `src.count(needle) == 1` before a run, never after a red.
# (2026-09-23: the literal-only Sorbet contracts added
# `note_sorbet_runtime_hazard` as the chain's last call; re-anchored.)
mutant M14 "$IDX" \
  '        self.note_sorbet_runtime_hazard(node);
        ruby_prism::visit_call_node(self, node);' \
  '        self.note_sorbet_runtime_hazard(node);
        let _ = &node;' \
  a_refinement_inside_a_module_new_block_is_silent \
  'recursion into call blocks: a refine inside `Module.new do ... end` is still a refine'

# The needle is the whole def frame, not just the recursion, because that
# is the mutant the control was proved against: with the frame gone,
# `self.nested` stays 0 inside a def body, which is what the named test
# distinguishes.
mutant M15 "$IDX" \
  '        self.nested += 1;
        self.def_locals.push(FxHashMap::default());
        ruby_prism::visit_def_node(self, node);
        self.def_locals.pop();
        self.nested -= 1;' \
  '        let _ = node;' \
  a_refinement_inside_a_method_body_is_silent \
  'recursion into method bodies: `def self.install; refine Integer do ... end; end` runs'

mutant M16 "$IDX" \
  '    scan.visit(&parse.node());' \
  '    let _ = &scan;' \
  a_refinement_stands_the_conclusive_lookup_down \
  'the scan runs at all: without it no refinement ever reaches the index'

mutant M17 "$IDX" \
  '            None => self.refined_unknown = true,' \
  '            None => {}' \
  an_unnamable_refinement_stands_every_class_down \
  'fail-closed on an unnamable target: "refined, unknown which" proves nothing about any class'

mutant M18 "$IDX" \
  '        index.refined_core.insert(name.clone());' \
  '        index.refined_unknown = true;
        let _ = name.clone();' \
  refining_an_unrelated_class_leaves_the_lookup_conclusive \
  'per-class, not blanket: refining Array must leave Integer + String accusing'

mutant M19 "$CHK" \
  '                || self.index.refined_core.contains(*n)
' \
  '' \
  a_refinement_stands_the_conclusive_lookup_down \
  'the checker actually reads refined_core, instead of collecting it for nothing'

mutant M20 "$CHK" \
  '        if self.index.core_mixin || self.index.refined_unknown || self.index.eval_polluted_unknown {' \
  '        if self.index.core_mixin || self.index.eval_polluted_unknown {' \
  an_unnamable_refinement_stands_every_class_down \
  'the checker reads refined_unknown too, the other half of the same read'

# ---- the eval-body suppression (this round) ---------------------------
#
# `Integer.class_eval("def +(o) = 'x'")` is the refinement problem in a
# second shape: methods arriving through a body no AST can read. Same
# plumbing, so the same eight decisions get their own mutants.

mutant M21 "$IDX" \
  '        if let Some(expanded) = index.expand_unresolved_alias_target(&nesting, &name) {
            index.eval_polluted_core.insert(expanded.trim_start_matches("::").to_string());
        }' \
  '        let _ = &nesting;' \
  an_aliased_or_unnamable_eval_receiver_stands_down \
  'alias resolution: `I = Integer; I.class_eval("def +(o) = ...")` prints "aliased" under MRI'

mutant M22 "$IDX" \
  '            None if string_body => self.eval_unknown = true,' \
  '            None if string_body => {}' \
  an_aliased_or_unnamable_eval_receiver_stands_down \
  'fail-closed on an unnamable receiver: `klass.class_eval(str)` proves nothing about any class'

mutant M23 "$IDX" \
  '        index.eval_polluted_core.insert(name.clone());' \
  '        index.eval_polluted_unknown = true;
        let _ = name.clone();' \
  a_string_eval_stands_its_receiver_down \
  'per-class, not blanket: evaling into Array must leave Integer + String accusing'

mutant M24 "$CHK" \
  '                || self.index.eval_polluted_core.contains(*n)
' \
  '' \
  a_string_eval_stands_its_receiver_down \
  'the checker reads eval_polluted_core, instead of collecting it for nothing'

mutant M25 "$CHK" \
  ' || self.index.eval_polluted_unknown {' \
  ' {' \
  a_bare_eval_stands_every_class_down \
  'the checker reads eval_polluted_unknown too, the other half of the same read'

mutant M26 "$IDX" \
  '            self.eval_unknown |= has_arg;' \
  '            let _ = ();' \
  a_bare_eval_stands_every_class_down \
  'the collector side of the bare-eval decision: `eval("class Integer; def +...")` runs clean'

mutant M27 "$IDX" \
  '        if !string_body && node.block().is_none() {' \
  '        if !string_body {' \
  a_block_class_eval_from_a_method_body_is_silent \
  'the BLOCK body counts too: a block in a method body is invisible to the walker contour'

mutant M28 "$IDX" \
  '        let Some(recv) = node.receiver() else {
            return self.nesting.last().cloned();
        };
        if recv.as_self_node().is_some() {
            return self.nesting.last().cloned();
        }
        const_path_str(&recv)' \
  '        let Some(recv) = node.receiver() else {
            return None;
        };
        if recv.as_self_node().is_some() {
            return None;
        }
        const_path_str(&recv)' \
  a_receiverless_eval_in_a_project_class_still_accuses \
  'receiverless `class_eval <<~RUBY` resolves to its enclosing class, never to "unknown"'


# ---- name-keyed pollution (round 6) -----------------------------------
#
# E0108 stopped asking "was this class touched?" and started asking "could
# the OPERATOR — or the conversion hook MRI consults for this pairing —
# have been added?". Every decision that answer rests on gets a mutant,
# and the two sides are separate decisions because MRI treats them
# differently (transcripts in `core.rs`).

mutant M29 "$CHK" \
  '        self.names_unpolluted(core_own_names(recv_cc), &[op])' \
  '        self.names_unpolluted(core_pollution_names(recv_cc), &[op])' \
  an_ancestors_operator_does_not_silence_but_its_coercion_hook_does \
  'the receiver side is the OWN class: `Object#+` never wins over `Integer#+` (MRI raises)'

mutant M30 "$CHK" \
  '            && self.names_unpolluted(core_pollution_names(arg_cc), &arg_keys)' \
  '            && self.names_unpolluted(core_own_names(arg_cc), &arg_keys)' \
  an_ancestors_operator_does_not_silence_but_its_coercion_hook_does \
  'the argument side is the whole ANCESTRY: `Object#coerce` makes `1 + "s"` print 2'

mutant M31 "$IDX" \
  '                self.push_keyed(None, vec![PollutionSource::Opaque]);' \
  '                let _ = ();' \
  a_bare_eval_silences_every_core_class \
  'the keyed half of the bare-eval decision, the one E0108 actually reads now'

mutant M32 "$IDX" \
  'fn pollution_is_unreadable(reason: Option<OpenReason>) -> bool {
    match reason {' \
  'fn pollution_is_unreadable(reason: Option<OpenReason>) -> bool {
    if matches!(reason, Some(OpenReason::ReopenedExternal)) {
        return true;
    }
    match reason {' \
  a_core_reopening_is_read_by_name \
  'the open REASON, not the flag: every core reopening is ReopenedExternal, so reading it as unreadable makes keying inert'

mutant M33 "$IDX" \
  '    let _ = depth;
    out.push(PollutionSource::Opaque);
    out
}' \
  '    let _ = depth;
    out
}' \
  an_unreadable_refine_body_statement_stands_its_class_down \
  'fail-closed on an unmodeled macro in a refine body, where no fragment can be read instead'

mutant M34 "$IDX" \
  '        // legal, needs exactly this.
        return vec![PollutionSource::Opaque];' \
  '        // legal, needs exactly this.
        return Vec::new();' \
  every_refine_target_form_is_read_by_name \
  'a block ARGUMENT is a body this file does not contain, not a body that defines nothing'

mutant M35 "$IDX" \
  '        .or_else(|| index.chase_alias_target_text(nesting, target));' \
  ';' \
  a_reopening_through_an_alias_is_read_by_name \
  'the second alias chaser: a reopening through an alias resolves in-project, so the first one bails'

mutant M36 "$IDX" \
  '            let bare_toplevel_include = node.name().as_slice() == b"include"
                && self.nesting.is_empty()
                && self.nested == 0;' \
  '            let bare_toplevel_include =
                node.name().as_slice() == b"include" && self.nesting.is_empty();' \
  an_include_inside_a_block_does_not_poison_object \
  'toplevel means outside blocks too: reading a spec block include as an Object mixin stood Object down on all three corpora'

mutant M37 "$IDX" \
  'fn attr_sources(name: &[u8], args: &[Node<'"'"'_>]) -> Vec<PollutionSource> {' \
  'fn attr_sources(name: &[u8], args: &[Node<'"'"'_>]) -> Vec<PollutionSource> {
    if true {
        let _ = (name, args);
        return vec![PollutionSource::Opaque];
    }' \
  a_literal_definer_macro_inside_an_eval_block_is_read_by_name \
  'literal `attr_*` names are READ: `attr_accessor :zz` adds zz/zz= and cannot touch the operator'

mutant M38 "$IDX" \
  '    for (name, module) in modules {' \
  '    for (name, module) in Vec::<(String, String)>::new() {
        let _ = &modules;' \
  a_mixin_inside_a_core_reopening_is_read_too \
  'the fragment half reads `include`d modules: `class String; include M` really gets M#coerce'

mutant M39 "$IDX" \
  '    resolve_fragment_pollution(index);' \
  '    let _ = &index;' \
  a_reopening_through_an_alias_is_read_by_name \
  'the fragment half runs at all: it is the only half that sees a reopening through an alias'

mutant M41 "$CORE" \
  '        CoreClass::Str => "to_str",' \
  '        CoreClass::Str => "coerce",' \
  the_coercion_hook_is_per_pairing \
  'the hook is per pairing: `String#+` converts with to_str, and `Object#to_str` really silences `"R$" + 2`'

mutant M42 "$CORE" \
  '    [hook, "method_missing", "respond_to_missing?"]' \
  '    [hook, hook, hook]' \
  method_missing_is_read_on_the_side_that_dispatches \
  'the missing-method pair answers for the conversion: with respond_to_missing?, `100 + "R$"` prints 2'

mutant M43 "$CHK" \
  '        if keys.iter().any(|k| self.index.polluted_any_class.contains(*k)) {
            return false;
        }' \
  '        if false {
            return false;
        }' \
  an_unnamable_refine_target_is_read_by_name \
  'the unnamable-target names count against every class: `refine klass do def +` is silence everywhere'

mutant M44 "$IDX" \
  '            if name == "method_missing" || name == "respond_to_missing?" {
                self.keyed_pollution.push((
                    Some("Object".to_string()),' \
  '            if false {
                self.keyed_pollution.push((
                    Some("Object".to_string()),' \
  a_toplevel_method_missing_stands_every_class_down \
  'a toplevel `def method_missing` lands on Object, which is in every argument ancestry'

mutant M45 "$IDX" \
  'fn pollution_is_unreadable(reason: Option<OpenReason>) -> bool {
    match reason {' \
  'fn pollution_is_unreadable(reason: Option<OpenReason>) -> bool {
    if matches!(reason, Some(OpenReason::UnknownClassBodyCall)) {
        return false;
    }
    match reason {' \
  an_unreadable_core_body_statement_stands_that_class_down \
  'an unmodeled macro in a core-class BODY: the fragment reason is the only reader left for it'

mutant M46 "$IDX" \
  '            self.push_keyed(target, vec![PollutionSource::Opaque]);' \
  '            let _ = ();' \
  every_string_eval_form_silences_its_receiver \
  'the keyed half of the string-eval decision: the body is unreadable, so that class stands down'

mutant M47 "$IDX" \
  '        if target.is_none() {
            return;
        }' \
  '        if false {
            return;
        }' \
  a_block_evaled_into_an_unnamable_receiver_still_accuses \
  'the limit on the block arm: `obj.instance_exec(&blk)` must not stand every class down (measured on mastodon)'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$CHK" "$BAK_CHK" && ok 'check.rs restored byte-identical' || bad 'check.rs NOT restored'
cmp -s "$IDX" "$BAK_IDX" && ok 'index.rs restored byte-identical' || bad 'index.rs NOT restored'
cmp -s "$CORE" "$BAK_CORE" && ok 'core.rs restored byte-identical' || bad 'core.rs NOT restored'
run_suite
if (( compiles )) && [[ -z $failing ]]; then
  ok 'post-restore suite green again'
else
  bad "post-restore suite not green: $(tr '\n' ' ' <<<"$failing")"
fi
trap - EXIT
rm -f "$BAK_CHK" "$BAK_IDX" "$BAK_CORE"

if (( fail )); then echo 'RESULT: FAIL (operand-types mutants)'; exit 1; fi
echo 'RESULT: PASS (operand-types mutants — 42 mutants, each accused by a named control)'

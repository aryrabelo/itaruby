#!/usr/bin/env bash
# Two-sided proof for the singleton-track class-body macro filings and
# for the gem-namespace key they sit beside. AGENTS.md, binding: the
# probes proving each side live next to the instrument, never only
# narrated — this file is where the mutation matrix lives instead of in
# a commit message.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/. A mutant
# that compiles with the suite green is a missing control, not a safe
# change (AGENTS.md): add the fixture, then count the mutant.
#
#   MUT-A  the `singleton_class.attr_accessor :a` receiver-spelling arm
#          stops filing (singleton-track family (a), second spelling)
#          -> sclass_call_attr_reader_arity_is_checked must fail: the
#             filed method was what carried E0102 to the wrong-arity call
#   MUT-B  `class_attribute`'s predicate filing is cut
#          -> class_attribute_is_filed_on_both_tracks must fail: the
#             POSITIVE side is the only control that can catch this one.
#             class_attribute_instance_predicate_false_removes_the_predicate
#             expects the predicate ABSENT, so a mutant that stops filing
#             it makes that fixture and the code agree vacuously — the
#             header used to name exactly that blind test as the control
#   MUT-C  the thread_mattr_* variants drop out of the mattr arm
#          -> thread_mattr_accessor_is_indexed_on_both_tracks must fail
#   MUT-D  gem_namespace_key degrades to exact equality (the 865fea9
#          camelize mechanism, removed one decision at a time)
#          -> rspec_reopen_matching_lockfile_gem_case_insensitively_stays_silent
#             must fail: `Rspec` from the lock never equals `RSpec` in
#             the code, the reopening looks like a complete project
#             definition, and the fixture's typo becomes accusable
#   MUT-E  the any_instance softening branch in soften_not_found is cut
#          -> any_instance_softens_when_a_mock_gem_is_locked must fail:
#             with rspec-mocks in the lock the lookup must soften to
#             Inconclusive; a cut returns it to conclusive NotFound
#   MUT-F  the `_exec` family drops out of the BODY_DEF_NAMES prefilter
#          -> every_dynamic_def_shape_opens_its_class must fail
#   MUT-G  the concern-edge gate on the `class_methods do` harvest is cut
#          -> a_non_concern_class_methods_block_invents_nothing must fail
#   MUT-H  the BLOCK spelling of `define_method` inside `class << self`
#          goes back to the instance track
#          -> sclass_define_method_called_on_an_instance_accuses must
#             fail: the name back on the instance track silences the
#             instance-side NoMethodError
#   MUT-I  the ARGUMENT spelling of `define_method`, same revert
#          -> sclass_define_method_and_aliases_are_indexed_on_the_singleton_track
#   MUT-J  `alias_method` inside `class << self`, same revert
#          -> sclass_define_method_and_aliases_are_indexed_on_the_singleton_track
#   MUT-K  the `alias foo bar` KEYWORD form, same revert
#          -> sclass_define_method_and_aliases_are_indexed_on_the_singleton_track
#   MUT-L  the literal def-body filing is cut
#          -> def_body_attr_accessor_arity_is_checked must fail
#   MUT-M  the fail-closed gate on an instance method body is cut
#          -> def_body_definer_in_an_instance_method_fails_closed
#   MUT-N  the receiver attribution is cut: a foreign literal definer
#          registers onto the enclosing class
#          -> def_body_foreign_literal_definer_stays_off_the_enclosing_class
#   MUT-O  the `send` unwrap on the REASON side is cut
#          -> def_body_send_wrapped_dynamic_name_opens_the_class
#   MUT-P  the `send` unwrap on the LITERAL side is cut
#          -> def_body_send_wrapped_attr_arity_is_checked
#   MUT-Q  the bare-call tail consult in soften_not_found is cut (ita-tail)
#          -> kernel_tail_raise_buckets_known_tail_and_still_silent must
#             fail: the raise flips back to conclusive NotFound and lands
#             in the residue bucket
#   MUT-R  the census's KnownTail LABEL is cut (ita-tail): the name still
#          softens, but the JSONL reads the unattributable
#          open(inconclusive_no_blocker) again
#          -> kernel_tail_raise_buckets_known_tail_and_still_silent must
#             fail on the known_tail assertion
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise
# resolve each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
IDX=crates/itaruby_semantic/src/index.rs
DIS=crates/itaruby_semantic/src/discovery.rs
CHK=crates/itaruby_semantic/src/check.rs
BAK_IDX=$ROOT/target/singleton-index.rs.orig
BAK_DIS=$ROOT/target/singleton-discovery.rs.orig
BAK_CHK=$ROOT/target/singleton-check.rs.orig
LOG=$ROOT/target/singleton-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$IDX" "$BAK_IDX"
cp "$DIS" "$BAK_DIS"
cp "$CHK" "$BAK_CHK"
restore() { cp "$BAK_IDX" "$IDX"; cp "$BAK_DIS" "$DIS"; cp "$BAK_CHK" "$CHK"; }
trap restore EXIT

# One test-binary run over every suite these mutants can be accused by.
compiles=0
failing=
# Sets `compiles` and `failing` in the CALLER's shell — never called in a
# command substitution, because a subshell's assignments die with it and
# "did not compile" would read as "green".
run_suite() {
  # `--no-fail-fast` is load-bearing, not hygiene: cargo stops after the
  # first failing test BINARY, so without it a mutant whose control lives
  # in a later suite reported "expected X to fail" while X had never run
  # (the defect operand-types-mutants.sh hit as M15 in round 6).
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test singleton_track --test gem_reopen_lockfile \
    --test mock_singleton_surface --test dark_singleton >"$LOG" 2>&1
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
  cmp -s "$IDX" "$BAK_IDX" && cmp -s "$DIS" "$BAK_DIS" \
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

mutant MUT-A "$IDX" \
  '                if call.receiver().is_some_and(|r| is_own_singleton_class(&r)) {' \
  '                if false {
                    let _ = frag_idx;' \
  sclass_call_attr_reader_arity_is_checked \
  'the receiver-spelling arm stops filing: E0102 rode in on the filed method'

mutant MUT-B "$IDX" \
  '                            if eff_predicate {
                                self.fragments[i]
                                    .singleton_methods
                                    .push(MethodDef::synthetic(format!("{attr}?"), 0, aspan));
                            }' \
  '' \
  class_attribute_is_filed_on_both_tracks \
  'the class_attribute predicate filing: the positive-side control names it'

mutant MUT-C "$IDX" \
  '                    "mattr_accessor" | "mattr_reader" | "mattr_writer" | "cattr_accessor"
                    | "cattr_reader" | "cattr_writer" | "thread_mattr_accessor"
                    | "thread_mattr_reader" | "thread_mattr_writer" | "thread_cattr_accessor"
                    | "thread_cattr_reader" | "thread_cattr_writer" => {' \
  '                    "mattr_accessor" | "mattr_reader" | "mattr_writer" | "cattr_accessor"
                    | "cattr_reader" | "cattr_writer" => {' \
  thread_mattr_accessor_is_indexed_on_both_tracks \
  'the thread variants drop out of the mattr arm'

mutant MUT-D "$DIS" \
  'pub fn gem_namespace_key(namespace: &str) -> String {
    let mut out = String::with_capacity(namespace.len());
    for c in namespace.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}' \
  'pub fn gem_namespace_key(namespace: &str) -> String {
    namespace.to_string()
}' \
  rspec_reopen_matching_lockfile_gem_case_insensitively_stays_silent \
  'the camelize key degrades to exact equality: lock `rspec` never equals code `RSpec`'

mutant MUT-E "$IDX" \
  '        if singleton && self.mock_singleton_methods.iter().any(|m| m == name) {
            return MethodLookup::Inconclusive;
        }' \
  '' \
  any_instance_softens_when_a_mock_gem_is_locked \
  'the any_instance softening branch is cut: the locked project keeps conclusive NotFound'

mutant MUT-F "$IDX" \
  'const BODY_DEF_NAMES: [&str; 12] = [
    "define_method",
    "define_singleton_method",
    "alias_method",
    "attr_reader",
    "attr_writer",
    "attr_accessor",
    "class_eval",
    "module_eval",
    "instance_eval",
    "instance_exec",
    "class_exec",
    "module_exec",
];' \
  'const BODY_DEF_NAMES: [&str; 9] = [
    "define_method",
    "define_singleton_method",
    "alias_method",
    "attr_reader",
    "attr_writer",
    "attr_accessor",
    "class_eval",
    "module_eval",
    "instance_eval",
];' \
  every_dynamic_def_shape_opens_its_class \
  'the prefilter drops the _exec family: their def bodies are never walked and the classes stay closed'

mutant MUT-G "$IDX" \
  '                    } else if call.name().as_slice() == b"class_methods"
                        && call.receiver().is_none()
                        && !scope.is_empty()
                        && is_concern_edge(&self.fragments[i].extends)
                    {' \
  '                    } else if call.name().as_slice() == b"class_methods"
                        && call.receiver().is_none()
                        && !scope.is_empty()
                    {' \
  a_non_concern_class_methods_block_invents_nothing \
  'the concern-edge gate on the class_methods harvest: a non-concern module must not get an invented ClassMethods surface'

mutant MUT-H "$IDX" \
  '                                // `define_method(:x) { ... }` — the
                                // BLOCK spelling, and the common one.
                                // Same track routing as the argument
                                // form below.
                                self.track(i, in_singleton).push(md);' \
  '                                self.fragments[i].methods.push(md);' \
  sclass_define_method_called_on_an_instance_accuses \
  'the BLOCK spelling of define_method goes back to the instance track: the instance-side NoMethodError goes silent again'

mutant MUT-I "$IDX" \
  '                                let mut md = MethodDef::synthetic(m, 0, span);
                                md.arity_unknown = true;
                                self.track(i, in_singleton).push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicDefineMethod),' \
  '                                let mut md = MethodDef::synthetic(m, 0, span);
                                md.arity_unknown = true;
                                self.fragments[i].methods.push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicDefineMethod),' \
  sclass_define_method_and_aliases_are_indexed_on_the_singleton_track \
  'the ARGUMENT spelling of define_method goes back to the instance track'

mutant MUT-J "$IDX" \
  '                                let mut md = MethodDef::synthetic(m, 0, span);
                                md.arity_unknown = true;
                                self.track(i, in_singleton).push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicAliasMethod),' \
  '                                let mut md = MethodDef::synthetic(m, 0, span);
                                md.arity_unknown = true;
                                self.fragments[i].methods.push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicAliasMethod),' \
  sclass_define_method_and_aliases_are_indexed_on_the_singleton_track \
  'alias_method inside class << self goes back to the instance track'

mutant MUT-K "$IDX" \
  '                        md.arity_unknown = true;
                        self.track(i, in_singleton).push(md);
                    }
                }
            }' \
  '                        md.arity_unknown = true;
                        self.fragments[i].methods.push(md);
                    }
                }
            }' \
  sclass_define_method_and_aliases_are_indexed_on_the_singleton_track \
  'the `alias foo bar` KEYWORD form goes back to the instance track'

mutant MUT-L "$IDX" \
  '        let mut md = MethodDef::synthetic(lit.name, lit.required, span);
        md.arity_unknown = lit.arity_unknown;
        self.track(i, lit.singleton).push(md);' \
  '        let mut md = MethodDef::synthetic(lit.name, lit.required, span);
        md.arity_unknown = lit.arity_unknown;
        let _ = (i, md);' \
  def_body_attr_accessor_arity_is_checked \
  'the literal def-body filing is cut: the class goes back to CLOSED with none of the names it installs, and the arity E0102 that rode on the filed reader disappears'

mutant MUT-M "$IDX" \
  '        if !self_is_the_class {
            self.open_class(i, OpenReason::DynamicDefineMethod);
            return;
        }' \
  '        let _ = self_is_the_class;' \
  def_body_definer_in_an_instance_method_fails_closed \
  'the fail-closed gate on an INSTANCE method body is cut: a name only one object answers to gets filed as the class surface'

mutant MUT-N "$IDX" \
  '        let DefTarget::Enclosing = target else {
            if let DefTarget::Named(path) = target {
                let oi = self.fragment_idx_for(&path, nesting);
                self.open_class(oi, OpenReason::DynamicDefineMethod);
            }
            return;
        };' \
  '        let _ = (&target, nesting);' \
  def_body_foreign_literal_definer_stays_off_the_enclosing_class \
  'the receiver attribution is cut: `Other.define_method(:x)` registers onto the ENCLOSING class, the one shape this step must never do'

mutant MUT-O "$IDX" \
  '        b"send" | b"public_send" | b"__send__" => args
            .first()
            .and_then(literal_method_name)
            .and_then(|inner| body_def_reason_named(inner.as_bytes(), &args[1..])),' \
  '' \
  def_body_send_wrapped_dynamic_name_opens_the_class \
  'the send unwrap on the REASON side is cut: `send(:define_method, key)` reads as an ordinary unknown send and the class stays closed with a surface it cannot enumerate'

mutant MUT-P "$IDX" \
  '        b"send" | b"public_send" | b"__send__" => match args.first().and_then(literal_method_name)
        {
            Some(inner) => body_def_literals_named(inner.as_bytes(), &args[1..]),
            None => Vec::new(),
        },' \
  '' \
  def_body_send_wrapped_attr_arity_is_checked \
  'the send unwrap on the LITERAL side is cut: `send(:attr_accessor, :mode)` files nothing and the arity E0102 disappears'

mutant MUT-Q "$IDX" \
  '            core::kernel_object_singleton_method(name) || core::kernel_bare_call_method(name)' \
  '            core::kernel_object_singleton_method(name)' \
  kernel_tail_raise_buckets_known_tail_and_still_silent \
  'the bare-call tail consult is cut: the bare raise goes back to conclusive NotFound and lands in the residue bucket — a prospective FALSE E0101 the inventory existed to kill'

mutant MUT-R "$CHK" \
  '                        let verdict = if kernel_bare_call_method(&name) {
                            DarkVerdict::KnownTail
                        } else {' \
  '                        let verdict = if false && kernel_bare_call_method(&name) {
                            DarkVerdict::KnownTail
                        } else {' \
  kernel_tail_raise_buckets_known_tail_and_still_silent \
  'the census KnownTail LABEL is cut: the raise still softens (silence preserved) but the JSONL reads the unattributable open(inconclusive_no_blocker) again — the audit trail is the thing that broke'

mutant MUT-S "$IDX" \
  '                            if treated_as_singleton {
                                self.fragments[i].singleton_methods.push(nd.md);' \
  '                            if false && treated_as_singleton {
                                self.fragments[i].singleton_methods.push(nd.md);' \
  nested_def_self_arity_is_checked \
  'the nested-def singleton filing is cut: the block-installed def self.fetch_data never lands on the class-object track, the arity E0102 disappears, and the shape goes back to reading as residue — a prospective FALSE E0101 the moment the track arms'
mutant MUT-T "$CHK" \
  '            let saved_dark = self.dark;
            self.dark = false;' \
  '            let saved_dark = self.dark;' \
  census_foreign_body_walk_records_nothing_in_the_host \
  'the census stand-down in the nested cross-file body walk is cut: method_return re-emits foreign-span records into the host file — the ghost/duplicate/line-0 corruption measured on discourse before ita-census'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$IDX" "$BAK_IDX" && cmp -s "$DIS" "$BAK_DIS" && cmp -s "$CHK" "$BAK_CHK" \
  && ok 'index.rs, discovery.rs and check.rs restored byte-identical' \
  || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then
  bad "post-restore suite not green: $(tr '\n' ' ' <<<"$failing")"
else
  ok 'post-restore suite green again'
fi
trap - EXIT
rm -f "$BAK_IDX" "$BAK_DIS" "$BAK_CHK"

if (( fail )); then echo 'RESULT: FAIL (singleton mutants)'; exit 1; fi
echo 'RESULT: PASS (singleton mutants)'

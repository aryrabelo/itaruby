#!/usr/bin/env bash
# Two-sided proof for bead H (onda 2): `def self.extended(base)` — the
# installs a hook performs on the object doing the `extend` land on the
# EXTENDER, on the surface they really reach. AGENTS.md, binding: the
# probes proving each side live next to the instrument, never only
# narrated — this file is where the mutation matrix lives instead of in a
# commit message.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/extended_hook.rs.
# A mutant that compiles with the suite green is a missing control, not a
# safe change (AGENTS.md): add the fixture, then count the mutant.
#
#   MUT-A  the `delegate` arm stops filing (hook names nothing)
#          -> delegate_hook_install_is_exactly_the_named_method must fail
#   MUT-B  the `define_method` arm stops filing
#          -> define_method_hook_install_resolves_silently must fail
#   MUT-C  the `class_eval` family stops being walked
#          -> class_eval_block_hook_install_resolves_silently must fail
#   MUT-D  the `def`s inside a literal `class_eval` block stop being read
#          -> class_eval_block_hook_install_resolves_silently must fail
#   MUT-E  `def base.x` is filed on the INSTANCE surface
#          -> singleton_def_hook_keeps_the_instance_surface_closed must
#             fail: a singleton method is one surface away from the call
#   MUT-F  the `send` family drops out of the unreadable installers
#          -> send_hook_opens_the_extender must fail
#   MUT-G  `instance_eval`/`instance_exec` drop out of the unreadable set
#          -> instance_eval_hook_opens_the_extender must fail
#   MUT-H  a `class_eval` argument (a string body) stops failing closed
#          -> string_class_eval_hook_opens_the_extender must fail
#   MUT-I  a non-literal `define_method` stops failing closed
#          -> dynamic_define_method_hook_opens_the_extender must fail
#   MUT-J  a non-literal `delegate` stops failing closed
#          -> dynamic_delegate_hook_opens_the_extender must fail
#   MUT-K  `hook_param_read` drops the NAME comparison (any local read
#          counts as the hook's parameter)
#          -> foreign_receiver_hook_installs_nothing must fail
#   MUT-L  `apply_extended_hooks` stops being wired into project_index
#          -> delegate_hook_install_is_exactly_the_named_method must fail
#   MUT-M  the merged class stops carrying the instance installs (the
#          transport between the two phases)
#          -> delegate_hook_install_is_exactly_the_named_method must fail
#   MUT-N  an install is filed with a KNOWN arity (0) instead of
#          `arity_unknown` — a delegated method takes what its target does
#          -> delegate_hook_install_is_exactly_the_named_method must fail:
#             the fixture's one-argument call becomes an E0102
#   MUT-O  the fail-closed verdict stops reaching the extender
#          -> send_hook_opens_the_extender must fail
#   MUT-P  `def self.extended` stops being harvested at all
#          -> delegate_hook_install_is_exactly_the_named_method must fail
#   MUT-Q  a non-symbol `delegate` argument is read as "nothing to file"
#          instead of unreadable
#          -> dynamic_delegate_hook_opens_the_extender must fail
#   MUT-R  a name-rewriting `delegate` keyword (`prefix:`) stops being
#          rejected
#          -> prefix_delegate_hook_opens_the_extender must fail
#   MUT-T  the opaque flag stops travelling from the fragment to the class
#          -> send_hook_opens_the_extender must fail
#
# PREFLIGHT=1 proves every anchor matches EXACTLY ONCE — the harness's own
# semantics, `src.count(needle)`, counted before any suite run is paid for
# — and stops there. Run it after any edit to a needle.
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise
# resolve each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
IDX=crates/itaruby_semantic/src/index.rs
BAK_IDX=$ROOT/target/extended-hook-index.rs.orig
LOG=$ROOT/target/extended-hook-mutants-test.txt
PREFLIGHT=${PREFLIGHT:-}

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$IDX" "$BAK_IDX"
restore() { cp "$BAK_IDX" "$IDX"; }
trap restore EXIT

# One test-binary run over the suite these mutants can be accused by.
compiles=0
failing=
# Sets `compiles` and `failing` in the CALLER's shell — never called in a
# command substitution, because a subshell's assignments die with it and
# "did not compile" would read as "green".
run_suite() {
  # `--no-fail-fast` is load-bearing, not hygiene: cargo stops after the
  # first failing test BINARY, so without it a mutant whose control lives
  # in a later suite reported "expected X to fail" while X had never run.
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test extended_hook >"$LOG" 2>&1
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
  if [[ -n $PREFLIGHT ]]; then
    ok "$label anchor matches exactly once"
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
  cmp -s "$IDX" "$BAK_IDX" || bad "$label source NOT restored byte-identical"
}

if [[ -z $PREFLIGHT ]]; then
  echo '--- baseline (shipped source)'
  run_suite
  if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
  if [[ -n $failing ]]; then
    echo "ABORT: baseline is not green — failing: $(tr '\n' ' ' <<<"$failing")"
    exit 1
  fi
  ok 'baseline green (every read arm files, every control and fail-closed fixture holds)'
else
  echo '--- anchor preflight (every needle must match exactly once)'
fi

mutant MUT-A "$IDX" \
  '            "delegate" => match hook_delegate_names(call) {
                Some(names) => {
                    for (n, span) in names {
                        self.fragments[i].hook_instance_installs.push((n, span));
                    }
                }
                None => self.fragments[i].hook_installs_opaque = true,
            },' \
  '            "delegate" => {}' \
  delegate_hook_install_is_exactly_the_named_method \
  'the delegate arm stops filing: the hook names nothing'

mutant MUT-B "$IDX" \
  '            "define_method" => match hook_define_method_name(call) {
                Some(n) => {
                    let loc = call.location();
                    self.fragments[i]
                        .hook_instance_installs
                        .push((n, (loc.start_offset(), loc.end_offset())));
                }
                None => self.fragments[i].hook_installs_opaque = true,
            },' \
  '            "define_method" => {}' \
  define_method_hook_install_resolves_silently \
  'the define_method arm stops filing'

mutant MUT-C "$IDX" \
  '            "class_eval" | "module_eval" | "class_exec" | "module_exec" => {
                self.harvest_extended_eval_block(i, call);
            }
' \
  '' \
  class_eval_block_hook_install_resolves_silently \
  'the class_eval family stops being walked: its literal block is never read'

mutant MUT-D "$IDX" \
  '        if let Some(def) = stmt.as_def_node() {
            if def.receiver().is_none() {
                let name = String::from_utf8_lossy(def.name().as_slice()).into_owned();
                self.fragments[i].hook_instance_installs.push((name, span_of(stmt)));
            }
            return;
        }' \
  '        if stmt.as_def_node().is_some() {
            return;
        }' \
  class_eval_block_hook_install_resolves_silently \
  'the bare `def`s inside a literal class_eval block stop being read'

mutant MUT-E "$IDX" \
  '                self.fragments[i].hook_singleton_installs.push((name, span_of(stmt)));' \
  '                self.fragments[i].hook_instance_installs.push((name, span_of(stmt)));' \
  singleton_def_hook_keeps_the_instance_surface_closed \
  '`def base.x` is filed on the INSTANCE surface: the real singleton install silences the instance-side NoMethodError'

mutant MUT-F "$IDX" \
  '            "send" | "public_send" | "__send__" | "instance_eval" | "instance_exec" => {
                self.fragments[i].hook_installs_opaque = true;
            }' \
  '            "instance_eval" | "instance_exec" => {
                self.fragments[i].hook_installs_opaque = true;
            }' \
  send_hook_opens_the_extender \
  'the send family drops out of the unreadable installers: a runtime-named define_method reads as an ordinary unknown call'

mutant MUT-G "$IDX" \
  '            "send" | "public_send" | "__send__" | "instance_eval" | "instance_exec" => {
                self.fragments[i].hook_installs_opaque = true;
            }' \
  '            "send" | "public_send" | "__send__" => {
                self.fragments[i].hook_installs_opaque = true;
            }' \
  instance_eval_hook_opens_the_extender \
  'instance_eval/instance_exec drop out: a define_method inside that block reaches the instance surface it no longer opens'

mutant MUT-H "$IDX" \
  '        if call.arguments().is_some() {
            self.fragments[i].hook_installs_opaque = true;
            return;
        }' \
  '        if call.arguments().is_some() {
            return;
        }' \
  string_class_eval_hook_opens_the_extender \
  'a class_eval argument stops failing closed: a string body installs whatever it says'

mutant MUT-I "$IDX" \
  '            "define_method" => match hook_define_method_name(call) {
                Some(n) => {
                    let loc = call.location();
                    self.fragments[i]
                        .hook_instance_installs
                        .push((n, (loc.start_offset(), loc.end_offset())));
                }
                None => self.fragments[i].hook_installs_opaque = true,
            },' \
  '            "define_method" => match hook_define_method_name(call) {
                Some(n) => {
                    let loc = call.location();
                    self.fragments[i]
                        .hook_instance_installs
                        .push((n, (loc.start_offset(), loc.end_offset())));
                }
                None => {}
            },' \
  dynamic_define_method_hook_opens_the_extender \
  'a non-literal define_method stops failing closed: the name it installed is never read'

mutant MUT-J "$IDX" \
  '            "delegate" => match hook_delegate_names(call) {
                Some(names) => {
                    for (n, span) in names {
                        self.fragments[i].hook_instance_installs.push((n, span));
                    }
                }
                None => self.fragments[i].hook_installs_opaque = true,
            },' \
  '            "delegate" => match hook_delegate_names(call) {
                Some(names) => {
                    for (n, span) in names {
                        self.fragments[i].hook_instance_installs.push((n, span));
                    }
                }
                None => {}
            },' \
  dynamic_delegate_hook_opens_the_extender \
  'a non-literal delegate stops failing closed: the names it installed are never read'

mutant MUT-K "$IDX" \
  'fn hook_param_read(node: &Node<'\''_>, pname: &str) -> bool {
    node.as_local_variable_read_node()
        .is_some_and(|read| String::from_utf8_lossy(read.name().as_slice()) == pname)
}' \
  'fn hook_param_read(node: &Node<'\''_>, _pname: &str) -> bool {
    node.as_local_variable_read_node().is_some()
}' \
  foreign_receiver_hook_installs_nothing \
  'the NAME comparison in hook_param_read is dropped: any local read counts as the hook parameter'

mutant MUT-L "$IDX" \
  '    apply_extended_hooks(&mut index);
' \
  '' \
  delegate_hook_install_is_exactly_the_named_method \
  'the pass stops being wired into project_index: nothing ever files'

mutant MUT-M "$IDX" \
  '    for (name, span) in &frag.hook_instance_installs {
        class.hook_instance_installs.push((name.clone(), *span, file));
    }
' \
  '' \
  delegate_hook_install_is_exactly_the_named_method \
  'the merged class stops carrying the instance installs: the harvest never reaches the second phase'

mutant MUT-N "$IDX" \
  '        arity_unknown: true,
        abstract_stub: false,
        file,
        def_span: span,' \
  '        arity_unknown: false,
        abstract_stub: false,
        file,
        def_span: span,' \
  delegate_hook_install_is_exactly_the_named_method \
  'an install is filed with a known arity: a delegated method really takes whatever its target takes'

mutant MUT-O "$IDX" \
  '    if opaque {
        merge_open(index, extender, OpenReason::EvalOrSend);
    }' \
  '    let _ = opaque;' \
  send_hook_opens_the_extender \
  'the fail-closed verdict stops reaching the extender'

mutant MUT-P "$IDX" \
  '                        if name == "extended" {
                            self.harvest_extended_hook(i, &def);
                        }
' \
  '' \
  delegate_hook_install_is_exactly_the_named_method \
  'def self.extended stops being harvested at all'

mutant MUT-Q "$IDX" \
  '    let Some(kw) = arg.as_keyword_hash_node() else { return true };' \
  '    let Some(kw) = arg.as_keyword_hash_node() else { return false };' \
  dynamic_delegate_hook_opens_the_extender \
  'a non-symbol delegate argument reads as "nothing to file" instead of unreadable'

mutant MUT-R "$IDX" \
  '    !kw.elements().iter().all(|el| {
        el.as_assoc_node().is_some_and(|assoc| {
            assoc.key().as_symbol_node().is_some_and(|k| {
                NAME_KEEPING.contains(&String::from_utf8_lossy(k.unescaped()).as_ref())
            })
        })
    })' \
  '    !kw.elements().iter().all(|_| true)' \
  prefix_delegate_hook_opens_the_extender \
  'a name-rewriting delegate keyword stops being rejected: the positional symbols are filed as THE install'

mutant MUT-T "$IDX" \
  '    class.hook_installs_opaque |= frag.hook_installs_opaque;
' \
  '' \
  send_hook_opens_the_extender \
  'the opaque flag stops travelling from the fragment to the class'

if [[ -n $PREFLIGHT ]]; then
  restore
  cmp -s "$IDX" "$BAK_IDX" && ok 'index.rs restored byte-identical' || bad 'source NOT restored'
  trap - EXIT
  rm -f "$BAK_IDX"
  if (( fail )); then echo 'RESULT: FAIL (extended-hook anchor preflight)'; exit 1; fi
  echo 'RESULT: PASS (extended-hook anchor preflight only — no mutant was run)'
  exit 0
fi

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$IDX" "$BAK_IDX" && ok 'index.rs restored byte-identical' || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then
  bad "post-restore suite not green: $(tr '\n' ' ' <<<"$failing")"
else
  ok 'post-restore suite green again'
fi
trap - EXIT
rm -f "$BAK_IDX"

if (( fail )); then echo 'RESULT: FAIL (extended-hook mutants)'; exit 1; fi
echo 'RESULT: PASS (extended-hook mutants)'
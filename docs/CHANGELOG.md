# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Public baselines regenerated for wave 2 (commit `600bc36`): rails 6 → 1
  error, discourse 7 → 3, mastodon unchanged — **0 new lines on every repo**,
  and the seven audit rows this closes bring the public corpora to **0 false
  positives** (4 errors, all true positives already recorded). Every removed
  line is marked CLOSED in `scripts/public-baseline/README.md`, which also
  records the finding that closes bead D by measurement rather than by argument:
  `vbulletin3.rb:1339`, D's acceptance criterion, is among the lines its sibling
  suppressions removed (`git log -S vbulletin3` shows it entered the baseline
  only at `0e64c5e`), so D is dropped exactly as bead A was. The wall column is
  a load reading, not a criterion median: the first pass measured rails 3.420 s
  and discourse 24.046 s against ceilings of 3 s and 10 s on a machine at load
  8.8–22, and a same-window A/B of five alternating runs per binary gave rails
  **1.544 s (wave 2) vs 1.606 s (parent `a0492ad`) — 0.96x, no regression**. The
  public gate's time verdict is therefore flaky under fleet load and must be
  re-measured with the parent before it is believed; its ceilings are unchanged.
- Two harness defects found and fixed by measurement, both the same family — a
  red for the wrong reason. `scripts/operand-types-mutants.sh` M14 and M15 went
  INVALIDO (`anchor matched 0 times`) because wave 2 put `note_load_hook_base`
  between the recursion and its neighbour, and added `def_locals` to the def
  frame; both anchors were re-derived and re-counted with the harness's own
  `src.count(needle) == 1`, and a sweep over all 111 anchors across the nine
  harnesses now reports zero broken. `scripts/inference-bench-selftest.sh`
  aborted before its first case under `set -u` — `${extra[@]}` on an EMPTY array
  is an unbound-variable error in bash < 4.4, which is macOS's `/bin/bash` — so
  the bench had no guard at all while gate g reported FAIL for a reason that had
  nothing to do with the bench; the `+` form fixes it and all thirteen mutants
  now accuse.
- Gate 0b — the MRI interpreter precondition (learned 2026-09-19, binding,
  measured): a local gate run whose PATH put `/usr/bin` ahead of mise's shims
  ran gate a on ruby 2.6 and failed INSIDE an operand-type fixture
  (`refined_integer_plus_silent.rb … syntax error, unexpected '='`) — ~70
  minutes of gate c1 spent before the two mutant families that run that suite
  aborted on `baseline is not green`: a red for the wrong reason, and the guard
  that caught it was the harness's own baseline check, working correctly. The
  gauntlet now states the interpreter before anything runs it — `PASS ruby
  3.x`, or a FAIL naming version and path — so that verdict costs two seconds.
  Proved both sides: with `/usr/bin/ruby` 2.6 first it accuses, with mise's
  3.4.2 first it stays silent. A MISSING ruby is still not a failure: the MRI
  legs skip by design.
- Attributed-mixin family (bead ita-a8z, phase A): three suppression-only
  mechanisms that close rails' 160-site `Rails::AppBuilder` E0101 cluster with
  ZERO new diagnostics — rails 166 → 6 errors (`new=0`, `gone=160`), mastodon
  and discourse byte-identical, measured on the pinned trees (2026-09-19).
  Mechanism 1 opens the class a mixin call's RECEIVER names (`X.include(M)`
  with a literal constant, or the value of a resolvable expression) when `M`
  defines `method_missing`/`respond_to_missing?`; mechanism 2 reads a local
  assigned from a project method whose body is a ternary of constant paths
  (`builder_class = get_builder_class`); mechanism 3 files the names a literal
  list crossed with an interpolated `class_eval` string defines
  (`%w(a b).each { |m| class_eval <<-RUBY def #{m} ... }`). Keyed on the
  RECEIVER, never on "some module answers every name" — the receiver-blind
  form silenced 222/222 of rails' baseline E0101 (AGENTS.md). INSTANCE track
  only: `X.extend(M)` is deliberately not attributed, because the index's
  `open` flag is read by both lookups and the arm's only effect was silencing
  INSTANCE lookups `extend` never justifies (the three public corpora are
  byte-equal with it gone). Mechanism 3's marginal on those corpora is zero in
  the configuration that ships — it is kept for the shape its own fixture
  proves against MRI (a method-PARAMETER include receiver, where no
  receiver-keyed mechanism can attribute anything), and its earlier "95 sites"
  credit was it measured WITHOUT mechanism 2. The harvest files names only
  where the eval body really runs: `Other.class_eval` inside `class Bar` no
  longer invents a method on Bar. Two-sided proof in
  `scripts/mixin-attribution-mutants.sh` (gate c1): nine mutants, one decision
  each, every one accused by a NAMED test, source restored byte-identical with
  `cmp`, `INVALIDO` when an anchor stops matching. Ten fixtures in
  `testdata/mixin_attribution/`, every accusing one MRI-raised and every silent
  one MRI-clean (the cross-file pair loaded together), plus a two-file test for
  the cross-file resolution the phase-2 map exists for.
- Fase A/onda 2 (beads B, H, C, E, F): five more fail-closed suppressions, each
  closing a measured false positive on the public corpora, each proved
  two-sided. `run_load_hooks(<literal symbol>, <literal base>)` opens the base's
  INSTANCE surface as `EvalOrSend` — nobody showed this checker a `class_eval`
  into it — taking rails' `activesupport/test/lazy_load_hooks_test.rb` from
  three E0101 to zero (rails-head 6 → 3 errors, discourse byte-identical,
  mastodon unchanged; `scripts/lazy-load-mutants.sh`, 6 mutants). A
  `self.extended(base)` hook files what it installs on the extender's real
  surface — `base.delegate` with positional literal symbols, `base.define_method`
  with a literal, a literal `base.class_eval do ... end` body, `def base.x` on
  the SINGLETON surface where it lands — and opens the extender when the install
  is provably there but its name set is unreadable (send/instance_eval/string
  `class_eval`/dynamic `define_method` or `delegate`/`prefix:`); rails-head 3 → 2
  (`activemodel/test/cases/naming_test.rb:333`), the `route_set_test.rb:18` true
  positive stays (`scripts/extended-hook-mutants.sh`, 19 mutants). `respond_to?(:m)`
  / `(:m, true)` and `x.is_a?(Class|Module) && x < Base` suppress the call they
  prove, in the branch where the predicate held and in the right operand only,
  keyed on the resolved PATH rather than resolvability — rails legitimately
  reopens the core `Class`, and a resolvability bail made the guard fire on the
  file alone while missing in the merged project
  (`scripts/guard-narrowing-mutants.sh`, 10 mutants). A call that is the direct
  subject of `assert_raises`/`assert_raise` or of `expect { }.to raise_error`
  enters the existing suppressed mode, span-keyed so a nested call keeps firing
  (`scripts/asserted-raise-mutants.sh`, 6 mutants), and the rebindable-block
  guard moved above the `lookup_method` dispatch so the Found/arity path
  consults it too, its NotFound-arm copy deleted
  (`scripts/rebindable-guard-mutants.sh`, 3 mutants). All five families wired
  into gate c1. Bead A was DROPPED on re-measurement, not implemented: its two
  discourse lines do not exist on the pinned corpus
  (`migrations/tooling/scripts/benchmarks/` is absent at `eff62154`; the ledger
  already records both as GONE), and the receiverless-versus-explicit-receiver
  distinction it needs lives in `check.rs`, not in the index — its corpus
  acceptance is empty and any index-only softening would break the control that
  must keep accusing. The narrower fail-closed arm was kept over the brief's
  wider one after measuring both: identical corpus counts, and the wider form
  fabricates uncertainty from calls that install nothing.
- `scripts/gate-triage`: routes a finished gauntlet run to its next action,
  reading `target/gauntlet/digest.json` only. Deterministic rules over digest
  features carry the routing — perf red without a parent-revision measurement
  (re-measure before believing), corpus revision drift (content vs detection
  drift), corpus error drift (audit new diagnostics as true positives or
  revert), artifact absence (absence is never agreement), public drift
  (unaudited until the ledger has a verdict), loaded host (load1 > 8 marks
  every ceiling suspect) — and four optional Jev judgments
  (`typesafe/jev-1.13`, ~0.6 s and ~US$0.00005 per call, measured
  2026-09-18) refine the route without ever creating, dismissing or blocking
  one. The instrument is advisory only: it never gates, exits 4 fail-open on
  transport trouble, and the state it sends to the model is a proven pure
  function of the digest (the secrecy wall travels with the digest). Two-
  sided proof in `scripts/gate-triage-selftest.sh`, wired as gate c2c:
  eight fixture runs routed exactly (green routes nothing), ten cmp-guarded
  mutants each accused by its named guard, unknown answer keys rejected with
  exit 2, a byte ceiling on the green state (1723 B measured).
- `scripts/gate-digest`: the gate suite's verdict as ONE compact JSON
  (`target/gauntlet/digest.json`, 1791 B all-green) instead of the ~115 MB
  `target/gauntlet/` holds — per-gate status, one-line reason and numbers
  (test counts, warning ceiling `x <= y`, bench ms vs ceiling, mutants
  accused/total, corpus new/gone counts and an error-set hash), the machine
  load at gate start, and for a FAIL the exact artifact path plus the line
  numbers to open. `gauntlet-gates.sh` now records its transcript to
  `target/gauntlet/transcript.txt` and calls the digest on every exit path,
  including the two early build failures; the digest is a reporter and can
  never change the gate's exit code. Private corpus artifacts yield counts,
  hashes and line numbers only — never content (secrecy wall); a PASS whose
  evidence file is missing or empty is reported `FAIL "artifact absent"`,
  never PASS. Two-sided proof in `scripts/gate-digest-selftest.sh`, wired as
  gate c2b: four fixture gauntlet directories under
  `scripts/gate-digest-fixture/` plus five cmp-guarded mutants, each accused
  by its own named case with the green case proved still green.
- E0108 asks whether the OPERATOR was polluted, not whether the class was
  touched. The blanket question ("did anything reach this core class?")
  is what the closed-world lookup needs and not what an operator check
  needs, and the difference is measurable: across rails, mastodon and
  discourse, 248 methods are defined directly inside core-class
  reopenings and ZERO of them is an operator or a coercion hook. So every
  pollution source is now collected WITH the names it can define
  (`index.rs`'s `PollutionSource`: a reopening's merged fragment, a
  `refine` block, an `include`d module's method set, a literal
  `define_method`/`alias_method`/`attr_*`/`delegate` injection — all
  readable; a string eval, a class-body block, a dynamic name, an
  unresolvable module — all `Opaque`, standing that ONE class down; a
  bare `eval(<string>)` — still every class, for every name), and
  `check.rs`'s `core_ops_unpolluted` asks the two questions MRI actually
  answers.
  The two sides are asymmetric because ruby 3.4.2 is, and both
  transcripts are in `core.rs`: on the RECEIVER only its own class
  counts, since `class Object; def +(o); end` leaves `p 1 + "s"` raising
  while `class Integer; def +` and `Integer.prepend(P)` make it print;
  on the ARGUMENT the whole ancestry counts, since `Object#coerce`,
  `Kernel#coerce` and `String#coerce` each make `1 + "s"` print `2`
  — and the hook is per pairing, `to_str` for a String receiver
  (`Object#to_str` makes `"R$" + 2` print `"R$o"`), `coerce` for a
  numeric one, with `to_int` measured NOT on that path at all.
  `method_missing`+`respond_to_missing?` on the argument's class count
  too (they make `"a" + 1` print `"amm"`); on the receiver's they do not,
  and `class Integer; def method_missing` is now a true positive.
  What it buys, measured with a `p 1 + "s"` probe added to a copy of each
  public corpus: mastodon goes from silent to ACCUSING — its only core
  reopening is `Numeric#to_json_c14n`, which cannot change `+` — while
  rails and discourse stay silent, each for one measured reason that is
  now the only thing left in the way: a bare `eval(<runtime string>)`
  (62 sites in rails, 120 in discourse). Whether an unreadable runtime
  string should stand E0108 down at all, given the checker already
  tolerates the identical unmodeled risk from every undeclared gem in
  the Gemfile, is the one switch this change deliberately leaves to the
  owner. Two limits are deliberate and pinned by their own tests:
  `Integer.include M` where `M` defines `+` stays silent though MRI
  raises (`include` inserts below the class, so `Integer#+` still wins),
  and a block evaled into a receiver nobody can name records nothing
  (`obj.instance_exec(&blk)` is ordinary Ruby — treating it as
  fail-closed stood every core class down on mastodon).
  The blanket fields E0108 no longer reads are still the closed-world
  lookup's, so they got the controls they had been borrowing from the
  E0108 suite: eight new tests in
  `crates/itaruby_semantic/tests/core_conclusive.rs` prove a refinement,
  an aliased refinement, an unnamable refinement, a string eval, an
  aliased eval and a bare eval each stand the conclusive lookup down,
  with a per-class control and an unpolluted control. The mutation matrix
  grew to 42 (M29–M47) and two of its own defects were found by it: a
  body reader whose every verdict the merged fragment already gave was
  DELETED rather than kept as dead defence, and `run_suite` was missing
  `--no-fail-fast`, so a control in the second suite could be reported
  as passing while it had never run.


- Eleven curated public gem namespaces in
  `crates/itaruby_semantic/declarations/gems.rbi` (wave 14): karafka
  2.6.1 (`Karafka`, `Karafka::Admin`), money 7.1.1 (`Money::Currency`),
  flipper 1.4.2 (`Flipper::Actor` and the
  `Flipper::Adapters::ActiveRecord::Gate` owner chain), rails
  activestorage (`ActiveStorage::FileNotFoundError`) and activeresource
  6.2.0 (`ActiveResource::ConnectionError`,
  `ActiveResource::UnauthorizedAccess`). Every name was read in the gem's
  own public source before curation — the file and version of each are
  recorded in `gems.rbi`'s wave-14 comment — never inferred from a corpus,
  which only ever says a name is unresolved, not that it exists.
  Measured both directions: corpus-a 290 -> 251 E0104 (-39: 23 money, 6
  karafka, 6 activeresource, 3 flipper, 1 activestorage) with its single
  error hash byte-identical, corpus-b 52 -> 51, corpus-c unchanged (179
  warnings, its four error hashes exactly the baseline set), and
  rails/mastodon/discourse unchanged in BOTH directions — zero warnings
  silenced and zero added on the public corpora, so no declaration
  reaches past the gem it names. corpus-a's ceiling tightens 284 -> 251
  in the same commit (slack is debt); corpus-b's 51 is still above 48 and
  its ceiling is deliberately untouched.
  Two-sided proof in `crates/itaruby_semantic/tests/declarations.rs`:
  `wave14_nested.rb` exercises every new namespace in a plain reference
  AND in a `rescue` clause (three of the measured sites are rescues,
  where an unresolved name is a rescue that can never match), and
  `wave14_nested_control.rb` keeps four undeclared siblings — one under
  each curated namespace — warning E0104, so the entries stay exact
  paths and never become the prefix fallback this repo reverted.
  Deliberately out, documented as ceilings rather than fixed:
  `HTTParty::COMMON_NETWORK_ERRORS` (a VALUE constant; this file carries
  only namespaces) and five first-party private-gem sites, which ship in
  no public gem and so can never enter a public declaration file.
- Singleton track, class-level attribute macros (probe at de28b40 →
  588b5ed): the receiver spelling `singleton_class.attr_accessor :a`
  files on the class-object track — the largest rails residue family
  (ActiveSupport::Dependencies, ActionDispatch::ExceptionWrapper,
  ActiveModel::Translation, ~90 of the 121 explicit-receiver sites);
  `class_attribute` files reader/writer/`a?` (predicate unless a
  literal `instance_predicate: false`, instance sides behind the
  documented option chain, dynamic options read as "defines
  everything"); `thread_mattr_*`/`thread_cattr_*` join the mattr arm.
  Rails explicit-receiver residue 121 → 42.
- Singleton track, mocking gems: `X.any_instance` softens to
  `Inconclusive` while rspec-mocks or mocha is in the project's own
  `Gemfile.lock` (name-keyed in `soften_not_found`, never blanket; no
  curated declaration exists for either gem and declaring
  `Module#any_instance` would have softened every singleton lookup
  project-wide). Discourse explicit-receiver residue 160 → 14; rails,
  mastodon and the private corpus have neither gem in their locks and
  are untouched by construction.
- `scripts/singleton-mutants.sh`, running as gate c1: seven mutants,
  each removing exactly one load-bearing decision of the singleton
  track (receiver-spelling filing, class_attribute predicate, thread
  variants, the any_instance softening, the `_exec` prefilter family,
  the concern-edge gate on the `class_methods do` harvest) plus the
  `gem_namespace_key` camelize key, each accused by a NAMED test.
- The `class_methods do` harvest is gated on the concern edge: a
  non-concern module's block can no longer invent a closed
  `M::ClassMethods` surface (measured: 123 of 125 corpus sites carry
  `extend ActiveSupport::Concern` before the block in the same file).
- BODY_DEF_NAMES covers the `_exec` rebind family
  (`instance_exec`/`class_exec`/`module_exec`), and
  `dynamic_def_prefilter_covers_every_reacting_name` pins prefilter and
  `body_def_reason` name by name — the pin the doc comment had been
  naming without shipping.

- E0108, operator operand type mismatch: `price = 100; label = "R$
  #{price}"; price + label` is now an error, where the checker used to be
  silent (measured silent on build sha 7f289c1b). MRI raises
  `TypeError: String can't be coerced into Integer` on that line, and the
  diagnostic points at the operand, naming both types: ``error[E0108]:
  `+` on Integer expects a numeric operand, got String``. Covers
  `Integer`/`Float` `+ - * /` against a `String`/`nil` operand and
  `String#+` against an `Integer`/`nil` one.
  The proof required is deliberately doubled, because flow-sensitivity
  alone cannot see a reassignment BELOW the operator: the walker's env
  must already know the type AND every write to that local inside the
  same Ruby scope (`prove_operand_locals`) must be a literal of the same
  type. One disagreeing write, one `+=`/multi-assign/`for`/`rescue =>`
  binding, one shadowing block parameter, one parameter of the enclosing
  `def`, or one `eval` anywhere in the scope, and the site goes silent.
  `def`/`class`/`module`/`class << self` each get their own map, the way
  Ruby scopes locals.
  Nothing coercion-shaped is modeled: if the project or any declaration
  reopens either operand's core class, or REFINES it
  (`refine Integer do def +(o) ... end end`, see Fixed below), the site
  stays silent, through the same `core_class_unpolluted` signal E0101
  already uses — BOTH sides, since `coerce` lives on the argument's
  class. Both shapes really run clean under MRI (`class String; def
  coerce` prints `200`; `class Integer; def +` prints `joined:R$`),
  which is why accusing them would be a false positive on a working
  program.
  Fixtures in `testdata/operand_types/` (five accusing, four silent),
  each ending in a top-level driver so `ruby` really executes the body
  the checker read, with `mri_ground_truth_is_executed` asserting the
  outcome per fixture — `TypeError` on the same line E0108 blames, a
  clean exit, or a counted number of rescued `TypeError`s where itaruby
  is deliberately silent (the binding-form and parameter rules'
  accepted false negatives). Plus
  `crates/itaruby_semantic/tests/operand_types.rs` and
  `scripts/operand-types-mutants.sh`, the mutation matrix as a runnable
  script rather than a narrated one: twenty-four mutants (M1a/M1b,
  M2–M7, M13–M28), one decision removed at a time, each demanding a
  NAMED test fail — each half of the proof, each pollution gate, the
  `+`-only restriction, the severity, the parameter poisoning and every
  refinement and eval-body decision. It restores both sources
  byte-identical with
  `cmp` and prints `INVALIDO` instead of passing when an anchor stops
  matching. Wired as gate c1 of `scripts/gauntlet-gates.sh` together
  with `scripts/const-missing-mutants.sh`, which had shipped unwired —
  a probe nothing executes decays into narration.

- Singleton-track index knowledge, step 1 of the program pinned in
  `crates/itaruby_semantic/tests/singleton_lookup.rs`:
  `attr_reader`/`attr_writer`/`attr_accessor` inside `class << self` are
  now filed on the CLASS-OBJECT track instead of the instance track,
  where no singleton lookup ever looked. MRI agrees on both halves —
  `Config.endpoint` works and `Config.new.endpoint` raises
  `NoMethodError` — so the move is a correction, not a widening. What it
  makes observable is the arity that rides on a `Found` lookup:
  `Config.endpoint("x")` on an `attr_accessor` reader is now
  `error[E0102]`, and MRI raises `ArgumentError: wrong number of
  arguments (given 1, expected 0)` on that same line. Openness is
  untouched, so the residue this unlocks is knowledge banked for the
  singleton `NotFound` report, not a new diagnostic family.
  Step 0 of the same program measured that residue first, with an
  instrumented build, per corpus and bucketed by why the method really
  exists at runtime (histogram in `singleton_track.rs`'s header):
  rails 888 sites (703 with an explicit receiver), mastodon 58 (36),
  discourse 4803 (4109), corpus-c 1701 (2). `class << self` `attr_*`
  itself contributes ZERO of them today, which is why this step ships as
  banked knowledge plus its arity test rather than as a corpus delta.
  Fixtures in `testdata/singleton_track/`, all MRI-executable, plus
  `crates/itaruby_semantic/tests/singleton_track.rs`.

- Singleton-track step 2, family (b): `extend self` and
  `module_function` now put a module's instance methods on the module
  OBJECT, modeled as the `extend <own path>` edge `lookup_singleton`
  already walks. `extend self` used to open the module instead
  (`OpenReason::DynamicMixinArg`) — it is the one `extend` argument
  whose target is never in doubt. `module_function` was treated as a
  plain visibility modifier, which is why `ActionCable.server`
  (`module_function def server`, 49 measured sites) and
  `Mastodon::Version.user_agent` (bare modifier) were invisible.
  Residue re-measured on the same four corpora: this family went from
  68/14/196/0 sites to 0/0/0/0, and the totals from
  888/58/4803/1701 to 650/44/4576/1701 (explicit-receiver
  703/36/4109/2 to 478/22/3898/2). Every public error set and the
  private corpus-c set stayed byte-identical, as required of an
  index-only step.

- Singleton-track step 3, family (c): a module that
  `extend ActiveSupport::Concern` and defines a nested `ClassMethods`
  module now carries an `extends` edge to it, so those methods answer on
  every includer's class object — the idiom behind
  `AdminDashboardIndexData.fetch_cached_stats`
  (`StatsCacheable::ClassMethods`) and behind the concern half of the gap
  characterized in `singleton_lookup.rs`. It keys on the literal
  `ActiveSupport::Concern` edge and requires the nested path to exist in
  the index: no `ClassMethods`, no edge, never a guess. Residue for this
  family went 0/0/4/0 to 0/0/0/0; totals 650/44/4572/1701 (explicit
  478/22/3894/2). Public and corpus-c error sets byte-identical.

- Singleton-track step 3b: the `class_methods do ... end` spelling of a
  concern's class methods. ActiveSupport::Concern `const_set`s a real
  `ClassMethods` module from that block, so the block's `def`s are now
  harvested onto a synthetic `<concern>::ClassMethods` fragment carrying
  the same `extends` edge step 3 added for the written-out module — one
  mechanism, both spellings. Openness is preserved (the block still opens
  the concern), so this banks knowledge rather than closing anything.
  It does fix two false positives on rails: `E0104 unresolved constant
  ConcernTest::Baz::ClassMethods` at
  `activesupport/test/concern_test.rb:81,87` is gone, because the
  constant really exists at runtime — rails' own passing tests in that
  file assert it is the module extended onto the includer. The rails
  baseline is regenerated (1092 -> 1090 lines, 167 errors unchanged, 925
  -> 923 warnings) with the audit recorded in
  `scripts/public-baseline/README.md`; mastodon and discourse are
  byte-identical.

- Fixed: a `Gemfile.lock` gem whose namespace carries a capital the
  segment boundaries do not predict was invisible to
  `apply_gem_reopenings`, which compared the camelize GUESS by exact
  string equality (`rspec` -> `Rspec`, never `RSpec`). Both sides now
  reduce to `discovery::gem_namespace_key` — ASCII-lowercase, separators
  dropped — so `connection_pool`/`ConnectionPool`,
  `message_bus`/`MessageBus` and `activesupport`/`ActiveSupport` pair up.
  Measured by reopening site on the reference corpora before the fix:
  mastodon `ConnectionPool` (2 sites), discourse `MessageBus` (1).
  The `wikicloth`/`fastimage` override entries are deleted: the key
  absorbs them. What remains in the table is only the kind the key
  cannot derive — a namespace differing in LETTERS — and each entry is
  read in the gem's own source at the locked version:
  `kt-paperclip` 8.0.0 `lib/paperclip.rb:82` (`module Paperclip`) and
  `ruby-vips` 2.3.0 `lib/vips/image.rb:9` (`module Vips`).
  SEGMENT matching, the tempting generalization, is refused and pinned by
  a control test: measured on the same corpora it would have blinded
  `Api` (145 reopening sites in mastodon, via `elasticsearch-api`),
  `Auth` (16 in discourse, via `auth-sanitizer`), plus `Scheduler`,
  `Form`, `Event` and `Web` — all of them the project's own namespaces.
  All four corpora unchanged in the diagnostic direction: errors
  byte-identical, warnings 923/986/1890 and corpus-c 179 unmoved.
  CORRECTION (measured 2026-09-17, after the commit above): the same
  entry first claimed the singleton `NotFound` residue was "identical at
  650/44/4572/1701". That number came from a probe build made BEFORE the
  fix was committed — `git checkout feat/singleton-track -- crates/` in
  the probe worktree while the fix was still uncommitted, so the probe
  measured the unfixed comparison and the reported "identical" was the
  same number twice. Re-measured on a probe built from the fixed source,
  the residue DROPS: rails 650 -> 299, mastodon 44 -> 22, discourse
  4572 -> 1534, corpus-c 1701 -> 1701 (explicit receivers
  478/22/3894/2 -> 130/0/861/2). Discourse's 2861 `RSpec.*` sites are
  now zero, which is exactly what the fix predicted: `rspec` is among the
  305 namespaces discovery parses from that lock, and
  `apply_gem_reopenings` opens 2862 classes with `RSpec` in the set.
  corpus-c does not move for a narrower reason, also instrumented: its
  `Gemfile.lock` exists (one directory above the checked root, found by
  the upward walk) and maps 291 namespaces opening 3117 classes, but it
  declares no `rspec`-prefixed gem, so `RSpec` stays closed there — and
  its residue contains zero `RSpec` receivers anyway: 92 distinct
  receivers, 2 explicit-receiver sites out of 1701, essentially all
  receiverless self-sends inside open-shaped classes.

- Fixed: `scripts/public-gate.sh` reported "public errors match baseline
  exactly" for all three repos, in ~0.01s each, while measuring nothing.
  It never created `$ART`, so on a tree without `target/gauntlet` every
  redirection failed and `comm` compared two files that do not exist —
  agreement by absence. Reproduced against the committed script
  (0.013s/0.009s/0.009s, exit 0). It now `mkdir -p`s the artifact
  directory, deletes the per-repo artifacts it is about to write, and
  asserts each one exists and is non-empty before comparing; a missing or
  empty artifact is a FAIL that names the path, never a PASS. Wired as a
  fourth case in `scripts/instrument-mutants.sh` (gate c2b): the mutant
  with the guard removed reports agreement with no artifact on disk, the
  shipped script both measures for real and fails loudly on an unwritable
  artifact directory.
- `scripts/gauntlet-gates.sh` now exports
  `CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}`. Two worktrees on
  this machine share one `build.target-dir`, and the gate's own
  `$ROOT/target/release/ita` already assumed otherwise: the build went to
  the shared directory while every binary-consuming gate read a path that
  build never wrote.

- Singleton-track step N+1, shape (1): the singleton reached by NAME
  rather than by lexical position. `X.singleton_class.include/prepend M`
  now puts M on X's class-object track (the `extends` edge
  `lookup_singleton` already walks), `.singleton_class.extend M` opens X
  instead of guessing, and `class << X` with a constant expression walks
  its body as X's singleton so `def`/`attr_*` land there and anything
  else opens X. `FileScan` had seen the `prepend` call all along and
  filed it as an INSTANCE-track dynamic mixin — true of a plain
  `prepend`, wrong through `singleton_class` — which is why discourse's
  205 `DiscourseEvent.track_events` sites sat in the residue.
  A patch applies ONLY to a path the project really declares
  (`apply_singleton_patches`, a resolve-last pass reading `by_path.get`,
  never `intern`). The first version interned, and discourse's
  `TCPSocket.singleton_class.prepend` invented a closed, method-less
  `TCPSocket`: one new E0101 on `TCPSocket.new(...).close` at
  `spec/support/nginx_test_proxy.rb:145`, code that runs. That trap is
  now a test of its own.
  Residue (probe rebuilt from this exact source): rails 299 (130
  explicit) unchanged, mastodon 22 (0) unchanged, discourse
  1534 -> 1312 (861 -> 639 explicit), corpus-c 1701 (2) unchanged.
  `DiscourseEvent` drops from 224 sites to 2, both of them
  `DiscourseEvent.raise` — a `raise` inside `def self.trigger`'s rescue,
  i.e. Kernel, not a class method.

- Singleton-track step N+1, shapes (2)-(4): a class whose singleton
  surface cannot be proven closed now says so.
  Shape (2) is the one that mattered and the one that was a latent
  invariant #1 bug: `def` bodies are never walked, so a class that
  installs its class methods with `define_singleton_method(key)` inside
  `def self.*` looked CLOSED with none of those names in it — discourse's
  `GlobalSetting` (`app/models/global_setting.rb:5`, `:69`, `:262`), 275
  residue sites, silent today only because the `Ty::Class` `NotFound`
  arm is characterized silent. Bodies are now scanned for definition
  shapes (`define_method`/`define_singleton_method` with a non-literal
  name, `alias_method`, `attr_*` with a non-symbol, `class_eval`/
  `module_eval`/`instance_eval`), attributed BY RECEIVER: implicit/`self`
  (or `singleton_class`) opens the enclosing class, a literal constant
  opens that constant by name, a dynamic receiver opens nothing.
  Attribution is not cosmetic — attributing a nested implicit-receiver
  call inside `MountedHelpers.class_eval do ... end` to the enclosing
  class opened `ActionDispatch::Routing::RouteSet` and swallowed a
  baseline rails E0101.
  Shapes (3) `extend <non-constant>` and (4) singleton
  `method_missing`/`respond_to_missing?` were already handled; they now
  have MRI-executed fixtures and tests so a later narrowing cannot
  silently close those classes.
  Perf: walking every body unconditionally measured `check/project_index`
  at 10.71 ms against a 7.04 ms ceiling, so a substring prefilter over
  the def's own span runs first (the technique the `NotImplementedError`
  scan already used). 6.20 ms after, against 6.13 ms for the parent
  revision measured in the same window — the ceiling is NOT tightened:
  this change spends slack, it does not create it. The prefilter's first
  version omitted `instance_eval` and the probe caught it as 10 MISSING
  discourse findings; the list is now pinned by a behavioral test over
  one fixture class per shape.
  Residue (probe rebuilt from this exact source): rails 299 -> 279 (130
  -> 121 explicit), mastodon 22 (0) unchanged, discourse 1312 -> 1023
  (639 -> 430 explicit), corpus-c 1701 (2) unchanged.

- Singleton-track family (e): the stdlib's own class-object surface,
  generated. `declarations/stdlib_singletons.txt` carries 7180 unique
  `Namespace.method` pairs harvested from the Ruby runtime by
  `scripts/gen-stdlib-singleton-inventory.rb` — one `--disable-gems`
  subprocess per stdlib lib, exactly the shape
  `gen-stdlib-inventory.rb`/`gen-core-inventory.rb` already use, plus a
  base-process pass for the namespaces that exist before any `require`
  (`Kernel`, `Math`, `Process`, ...). Hash-locked by
  `L2.GENERATED_FILES_ARE_LOCKED`.
  The harvest is `singleton_methods(true)` minus everything
  `Module`/`Class` answer, not `singleton_methods(false)`: the first
  version missed `SecureRandom.uuid` outright, because SecureRandom gets
  its surface by extending `Random::Formatter` and defines nothing
  directly. Consumed by `soften_not_found` on the singleton track only,
  suppression only, and deliberately ungated on `require` — gating is
  the only direction that could turn silence into a diagnostic on code
  that runs.
  The two exclusions the generator applies (the `Module`/`Class`
  surface `core.rs` already models, and the ten core classes
  `core_inventory.txt` owns) are asserted against the COMMITTED file by
  `crates/itaruby_semantic/tests/stdlib_singletons.rs`, so a
  regeneration that widens the inventory fails the build instead of
  quietly doubling it. The consumer is pinned two-sided in the same
  file: `FileUtils.mkdir_p`/`SecureRandom.uuid`/`Kernel.rand` known,
  `FileUtils.mkdir_pp`/`SecureRandom.uuidd`/`Workspace.prepare` not.
  Residue: rails 279 (121 explicit) unchanged, mastodon 22 (0)
  unchanged, discourse 1023 -> 753 (430 -> 160 explicit), corpus-c 1701
  (2) unchanged. Perf `check/project_index` 5.86 ms against the 7.04 ms
  ceiling.

- An executable inference benchmark against Sorbet,
  `scripts/inference-bench.rb` (gate `scripts/inference-gate.sh`, guarded
  by `scripts/inference-bench-selftest.sh`, documented in
  `scripts/inference-bench-README.md`). Twelve self-contained cases, one
  directory each, on which MRI is the ground truth: every `accuse` case
  must really raise and every `silent` case must really run clean, so no
  row is a human's opinion. Scoreboard on 2026-09-17: 4 cases both
  checkers prove, 2 Sorbet proves and itaruby does not
  (`extend_singleton_typo`, `included_hook_class_method_typo` — recorded
  as our gaps, not omitted), 2 both correctly silent, and 3 silence rows
  that carry a positive control, a sibling fixture with the same dynamic
  shape plus a typo that MRI really raises, so a checker blind to the
  whole surface cannot score as "correctly silent".
  The claim is scoped, and the scope is printed with every run: zero
  gems, zero Tapioca/generated RBIs, zero itaruby curated declarations,
  every fixture pinned `# typed: true`, `srb 0.6.13437`. Neither tool
  gets its declaration pipeline, so nothing here measures either tool on
  a real application. **No timing, speed or throughput claim is made or
  measured.** On the 3 rows licensed to say "Sorbet cannot prove this
  without an annotation", the bench re-runs Sorbet on the SAME program
  (byte-identical, sha-checked) plus the sig or RBI a human or Tapioca
  would write, and measures it clean; rows with no such leg are reported,
  never claimed. This is not a parity benchmark.

- ActiveSupport's `mattr_accessor` family (`mattr_reader`,
  `mattr_writer`, and the `cattr_*` spellings of the same macro) is now
  indexed on BOTH tracks — the class-object accessor and, unless told
  otherwise, the instance accessor — honoring `instance_accessor:`,
  `instance_reader:` and `instance_writer: false`, since inventing an
  accessor the macro suppressed would hide a real `NoMethodError`. The
  name appeared zero times in `index.rs` before this.
  **Additive only, deliberately: the arm records the accessors and
  leaves the class exactly as open as it was.** Closing it was the first
  attempt and was measured and reverted the same day — on discourse,
  `TopicQuery`'s only class-body opener is
  `cattr_accessor :results_filter_callbacks`, so handling the macro
  closed the class and produced 28 new E0101 on methods that are real,
  installed from a plugin file by
  `add_to_class(:topic_query, :list_group_topics_assigned)`, which no
  index here can model. Closing a class adds no knowledge; it only
  unmasks what the index already cannot see. So the tests in
  `crates/itaruby_semantic/tests/mattr_accessor.rs` assert index CONTENT
  rather than diagnostics, the three public corpora move by exactly zero,
  and the price is pinned honestly in
  `testdata/mattr_accessor/typo_stays_silent_class_open.rb`: an
  instance-accessor typo stays silent (MRI raises) because the class
  stays open.

### Changed
- Public-corpus baselines regenerated for the attributed-mixin family: rails
  1100 → 940 lines (166 → 6 errors, **0 new**), mastodon and discourse
  byte-identical — their files do not appear in the diff at all, which is the
  measurement that the mechanism is receiver-keyed rather than a blanket
  softening. The 160 removed lines are the two Thor builder false-positive
  families the ledger had already recorded as such. The ledger carries the new
  section with the pinned shas, the gate's own wall times and the new totals
  (**13 = 8 FP + 4 TP + 1 inconclusive**); the ceilings are unchanged and the
  reason is stated (a wall reading is machine load, not a criterion median).
  `./scripts/public-gate.sh` matches all three baselines exactly.
- Publishing hygiene for the public launch: workspace package metadata
  (license/repository/homepage) inherited by every crate, the project
  site linked from the README, local scratch paths reworded out of
  comments, and a NOTICE for the third-party diff hunks embedded in the
  bug-replay candidates corpus.

### Fixed

- The singleton track files every method-defining form where it really
  lands, and one rails false positive goes with it. Four defects, found
  by review before the `Ty::Class` `NotFound` arm reports and each one a
  guaranteed invariant #1 violation the moment it does: inside
  `class << self`, `define_method` (block and argument spelling),
  `alias_method` and the `alias` keyword were filed on the INSTANCE
  track, where no singleton lookup ever looks and where they invent
  instance methods MRI does not have; a literal definer inside a `def`
  body registered NOTHING, so `def self.install;
  define_singleton_method(:ready?) { true }; attr_accessor :mode; end`
  left the class CLOSED without any of the names it installs
  (discourse's `GlobalSetting`); `send(:define_method, ...)` was not
  unwrapped in a def body although the core-pollution walker has
  unwrapped it since `definer_sources_named` was split; and an
  explicit-receiver literal (`Other.define_method(:x)`) produced no
  reason at all, leaving that class closed without the name — the shape
  behind rails' `ActionDispatch::Routing::RouteSet` E0101, a false
  positive this repository's own audit ledger had already recorded as
  one and which is now gone by correction (rails public baseline 1090 ->
  1089 lines; mastodon and discourse byte-identical). Attribution is by
  receiver and by whether `self` is provably the class: a `def self.x`
  body files the names, an instance body opens the class instead
  (`define_singleton_method` there lands on ONE object), a foreign
  constant receiver opens THAT class and never touches the enclosing
  one. Singleton residue re-measured with a probe built from this tree
  against the same probe on the parent revision: rails 42 (193),
  mastodon 0 (22), discourse 14 (605) — identical on both sides, so the
  flip stays blocked for the same reasons and nothing regressed.
  Two-sided: nine MRI-executed fixtures in `testdata/singleton_track/`,
  nine mutants (MUT-H..MUT-P) in `scripts/singleton-mutants.sh`, and the
  suite's MRI ground truth is now EXECUTED over all 39 fixtures instead
  of narrated in doc comments.
- E0108 no longer fires on a REFINED core operator — an invariant #1
  violation found by review and reproduced: `module IntPlus; refine
  Integer do def +(other) = "refined #{other}" end; end` + `using
  IntPlus` makes `1 + "s"` print `"refined s"` and exit 0 under MRI,
  and the checker accused it. A refinement creates neither a class
  fragment (`by_path`) nor a method-injection call (`core_mixin`), so
  every closed-world core lookup stayed conclusive; `index.rs` now
  collects refinements and `core_class_unpolluted` consults them, so the
  fix reaches every closed-world core lookup (E0101 included), not just
  E0108. Decisions, each with its own control: the mark is project-wide
  and ignores `using`'s lexical scope (tracking activation buys only
  false positives if the tracking is ever wrong, and there are zero
  `refine` calls across all four corpora); a target that cannot be named
  (`refine klass do`) stands EVERY core class down rather than none,
  because "some core class was refined, unknown which" is exactly the
  state where nothing is provable; and a named non-core target
  (`Foo::Integer`, or a project class) poisons nothing. Same silence for
  `refine ::Integer`, `refine(Integer)` and `refine Integer, &blk` — the
  last being free, since MRI rejects it with `ArgumentError: can't pass
  a Proc as a block to Module#refine`.
  A second review round measured three more clean-under-MRI shapes still
  accused, all fixed in one move: the collector is now `RefineScan`, a
  file-level prism `Visit` instead of an arm in the class/module-body
  walker, so a `refine` inside a METHOD body (`def self.install`),
  inside a `Module.new do ... end` block, or inside `module_eval` is
  seen; and the target is chased through constant ALIASES after the
  merge (`resolve_refined_core` + `expand_unresolved_alias_target`), so
  `I = Integer; refine I` marks `Integer`. `A = Array; refine A` still
  leaves `Integer + String` accusing — alias resolution is per-class,
  not a blanket.
  A third round closed the same hole in its other shape: an eval body.
  `Integer.class_eval("def +(o) = 'x'")` followed by `p 1 + "s"` prints
  `"x"` and exits 0 under MRI, and E0108 accused it — eleven such
  programs were measured accusing before this change, all now silent
  (`FileScan::note_opaque_eval`, `eval_polluted_core` /
  `eval_polluted_unknown`, consulted by the same
  `core_class_unpolluted`). The literal-receiver shape at toplevel or in
  a class body was already covered by `core_injection_call`'s
  project-wide `core_mixin`; what this adds is every contour that walk
  never visits — a method body, a block, a conditional — plus the two
  shapes a literal receiver hides: a receiver spelled as a constant
  ALIAS (`I = Integer; I.class_eval(...)`, chased in
  `resolve_eval_polluted_core`) and a receiver that cannot be named.
  What counts as a body nobody showed the checker: any positional
  argument to `class_eval`/`module_eval`/`instance_eval` (those three
  take a string and nothing else, so a literal, a heredoc, an
  interpolation or a variable are all bodies — and `instance_eval`
  really can define an INSTANCE method:
  `Integer.instance_eval("define_method(:+) { |o| 'ie' }")` prints
  `"ie"`), or a block on any of those or on
  `class_exec`/`module_exec`/`instance_exec`. The decisions, each with a
  control: a receiverless `class_eval <<~RUBY` resolves to its enclosing
  class (the Rails idiom), never to "unknown", so it poisons nothing
  when that class is a project class; an unnamable receiver with a
  STRING body (`klass.class_eval(str)`,
  `Object.const_get(x).class_eval(str)`) stands EVERY core class down,
  the `refined_unknown` decision again, while the same receiver with a
  BLOCK does not, because `x.instance_eval { ... }` is ordinary DSL code
  (25 sites in rails, 16 in discourse) that names no class at all; and
  `Array.class_eval(str)` leaves `Integer + String` accusing.
  `eval`/`Kernel.eval`/`binding.eval` with any argument stands every
  core class down: the string can define anything anywhere. E0108's
  locals map does not already cover that — it bails the scope it scans
  (`x = 1; eval("x = 'str'"); x + 1` was already silent), but two
  LITERAL operands need no local, and `eval(...); p 1 + "s"` accused.
  The price is measured and accepted: rails (62 `eval` sites with an
  argument, prism-counted) and discourse (120) carry the project-wide
  mark, and a block body that
  defines something else (`Integer.class_eval { def doubled = self * 2 }`)
  stops accusing — the same false negative the toplevel contour has
  always had — until round 6 keyed pollution by NAME and turned it into
  a true positive (see the entry above), which is why that test now
  reads `a_block_eval_body_that_defines_nothing_relevant_still_accuses`.
  Attributed afterwards rather than assumed: neither project had E0108
  alive BEFORE this change either — a `p 1 + "s"` probe added to each
  clone and checked with the parent revision's binary reports 0 E0108
  rows on all three public corpora, because `core_mixin` plus a project
  reopening of `Object`/`Kernel` already stood every core class down
  (mastodon, which has no unknown mark at all, is blocked by its
  reopening of `Numeric`). Parsing a literal eval body as a sub-program
  was then measured and rejected, see AGENTS.md's `Removed — do not
  reintroduce`. Eight new
  mutants (M21–M28) cover the collector, its fail-closed arms, the
  receiverless fallback and both new checker reads; M19/M20's anchors
  moved with `core_class_unpolluted`, whose two reads became three.

- `unresolved constant` (E0104) no longer fires inside a namespace that
  defines `self.const_missing`. Such a namespace autovivifies constants
  at runtime, so its surface is unknowable by construction and a
  diagnostic there reads absence of evidence as evidence of absence
  (invariant #1). Two halves, found separately: the QUALIFIED form
  (`Plugins::Markdown` where `Plugins` defines the hook), caught by the
  new inference bench as a live false positive on a program MRI runs
  clean; and the BARE form (`Widget` written inside the module that
  defines the hook), caught by review afterwards.
  The bare half is scoped by a measured fact about Ruby, not by
  reasoning: `const_missing` is called on the CREF, and only on the
  innermost one. With the hook on `Outer` and a bare `Widget` inside a
  nested `Outer::Inner`, MRI still raises
  `uninitialized constant Outer::Inner::Widget`. So the fix walks no
  cref chain and no ancestry — only the immediate lexical namespace —
  and it is suppression-only, never a new diagnostic.
  Six fixtures in `testdata/const_missing/`, each cross-checked against
  MRI in both directions (three exit 0 and are silent, three raise and
  still warn on the same line), plus
  `scripts/const-missing-mutants.sh`: MUT-A cuts the suppression and the
  false positive returns, MUT-B stops checking whose hook it is and both
  scope controls go silent, MUT-C cuts the bare-reference branch and
  only that false positive returns. `check.rs` is proved byte-identical
  after each.

- `scripts/instrument-mutants.sh` could not be trusted twice in a row.
  The gate passes a fixed lab path, so runs accumulated into it and the
  `replay-run-isolation` case failed on every invocation after the
  first; the lab is now recreated fresh at the start of each run. And a
  relative `INSTRUMENT_MUTANTS_LAB` resolved differently inside each
  case, which made every case report "did not reproduce" from an empty
  witness — indistinguishable from a passing instrument; it now fails
  closed with a named reason instead.

- The evidence instruments no longer lie about what they measured. Three
  defects, all live and silent until 2026-09-17, all in the scripts that
  produce the evidence the gates read:
  `scripts/gauntlet-gates.sh` printed `FAIL cargo build --release` and
  then ran every binary-consuming gate (mutation probe, corpus diff,
  navigation oracle, public corpus) against whatever stale
  `./target/release/ita` was on disk; it now stops at the failed build.
  `scripts/bug-replay/replay.sh` appended every invocation into one
  `results/<id>.jsonl` — the file held 33 lines for 10 unique pairs, with
  one id carrying verdicts from two different days, so a previous run's
  MISS read as today's recall; each invocation now owns
  `results/<run-id>/` with a `manifest.json` pinning the source sha, the
  tree's dirty state and the binary's SHA-256. And both `replay.sh` and
  `scripts/bug-replay/selftest.sh` built with a bare
  `cargo build --release`, which on a machine with a global
  `build.target-dir` writes somewhere other than the `$ROOT/target/release/ita`
  they then execute (`selftest.sh` skipped the build entirely when a
  binary already existed, while still printing "built from a checked
  cargo build --release"); both now build `--locked --target-dir
  "$ROOT/target"`, checked, before any capture.
  Proved two-sidedly by the new `scripts/instrument-mutants.sh`
  (gate c2b): each defect is re-injected as a mutant and must reproduce —
  the stale binary records its own execution, two runs collapse into one
  file, the unpinned build measures the stale binary — while the shipped
  scripts stay clean, with a `cmp`-style guard failing any mutation that
  did not apply and a positive control proving the harness itself is not
  blind.

- Open-class reasons now carry a precedence: `AbstractRaise` (the `raise
  NotImplementedError` stub idiom) is the WEAKEST reason and is replaced
  by any later, stronger one — a class-body delegate loop, `eval`/`send`,
  `method_missing`, a gem reopening. First-reason-wins let a class whose
  body opened with a raise stub and later looped
  `%i[...].each { delegate ..., to: :@lookup }` (Forwardable defines real
  methods at load time) masquerade as a pure abstract stub, so the
  abstract-raise lookup softening below turned 14 delegated names into 23
  false E0101s across discourse's `script/import_scripts/*` importers
  (measured on the pinned discourse sha before the precedence fix; the
  false positives never entered a baseline). The precedence applies at
  fragment recording, fragment merge, and both reopen force-open sites,
  and is pinned by `testdata/class_body_block/` (loop silent, loop-less
  control still accuses, and the raise-first/loop-after shape silent
  under the combined softening and firing under the precedence mutant).
- `undefined method` (E0101) silence around the abstract-raise idiom
  (`raise NotImplementedError` in a base method) is no longer blanket. A
  base class carrying the stub, and every lookup through it, used to be
  inconclusive no matter what; now the stub class is treated as closed
  for instance lookups — its own methods resolve (and arity-check, with a
  shadowed stub skipped when any descendant redefines the name) — and a
  resulting `NotFound` softens to inconclusive only when a member of the
  RECEIVER's subtree provably defines the name (`abstract_family_defines`:
  keys on the method name and the structural descendant set; pass-through
  only for members open solely via their own raise stub; any other open
  member, a `method_missing` member, an open mixin, or an
  unresolved/declared-external link under the receiver keeps silence).
  Measured: corpus-c +1 true positive (audited by reading the flagged
  code; hash `33aded51a652e5b7` reported for the ratchet owner),
  discourse +3 true positives (audited: `BulkImport::VBulletin5` x2,
  `BulkImport::Base` x1 — delegated names absent repo-wide), rails and
  mastodon byte-for-byte unchanged (the pinned rails sha already carries
  the upstream fix behind this idiom, and `Tags::SearchField`'s own
  ancestry keeps open helper modules, so the historical NameError stays
  correctly inconclusive there), fixture corpus: 9 shapes, two-sided
  under mutant.

- Top-level constant writes in cbase form (`::X = value`) are now indexed
  for resolution. The write arm previously only harvested the bare
  `X = value` shape and multi-segment `A::B = value` qualified writes, so
  a `::DB = connection` in one initializer left every later read of `DB`
  (bare or cbase) flagged as unresolved (E0104). The read side's
  unresolved-owner fallback also now keys single-segment cbase references
  by their simple name, matching how top-level constants are stored.
- Reads of a constant in the branch that only runs after `defined?(Const)`
  proved it exists no longer warn as unresolved (E0104): the then-branch
  of `if defined?(X)` / `defined?(X) ? X : fallback` and the else-clause
  of `unless defined?(X)`. Suppression is keyed to the exact guarded
  constant and scoped to exactly that branch — the opposite branch and
  any other constant still warn. Combinator predicates
  (`defined?(X) && y`) are deliberately not handled yet.
- `undefined method` (E0101) false positives from a dynamic-receiver
  `include`/`extend`/`prepend` — `builder_class.include(ActionMethods)`
  where the receiver is a local variable, invisible to static ancestry,
  but the mixed-in module is a literal constant. A `NotFound` method
  lookup now softens to inconclusive when the method name is directly
  defined by such a dynamically-mixed module, tracked separately for
  the instance (`include`/`prepend`) and singleton (`extend`) dispatch
  tracks and keyed only on the method name — never on the receiving
  class's identity or name, so it cannot resurrect a name-heuristic false
  negative. Measured against five real corpora: 41 rails + 7 discourse
  sites silenced, zero new diagnostics anywhere.
- `undefined method` (E0101) false positives from `.extend(Module)` on an
  instance-variable receiver: the existing widening (receiver type ->
  Unknown after `.extend`) only applied to a local-variable receiver.
  An ivar's widening now survives across method boundaries (any method
  can reassign an ivar, and the class's own `.extend` call is commonly
  in a different method than the later read — e.g. a test's `setup` vs.
  its `test_*` methods), and fires unconditionally rather than gated on
  closed-world mode: it can only ever suppress a diagnostic, never
  fabricate one, so it must not depend on a mode most real corpora never
  enable.
- `undefined method` (E0101) false positives from a `def` inside a
  class-body `begin`/`rescue`/`else`/`ensure` block used as one
  statement among several: the walker indexed both arms of `if`/`else`
  but never descended into `begin`'s arms at all, so an alternative
  method definition living in `rescue` (or `else`/`ensure`) was invisible.
  Now walks every arm the same "conservative: walk every arm" way
  `if`/`else` already does — for `def`s, `CONST = ...` writes, and
  `require` calls alike, since the walk is the same general statement
  walk `if`/`else` uses, not a `def`-only special case.

### Known gaps

Recorded because they were measured and left open, not because they are
scheduled. Both are characterized — as CURRENT behavior, never desired —
in `crates/itaruby_semantic/tests/singleton_lookup.rs`, so the day one
closes, a test says so.

- Two certain `NoMethodError`s on the singleton track go unreported: a
  typo on a method an `extend`ed module puts on the class object, and a
  typo on a class method a Rails concern injects through
  `def self.included(base); base.extend(ClassMethods); end`. Both are
  rows in the inference bench, where Sorbet proves them and itaruby does
  not.
  An attempt to close them was built and measured on 2026-09-17, and
  **reverted**: making a `NotFound` on the singleton track a real
  diagnostic produced **+703 new diagnostics on rails** and **+4109 on
  discourse**, overwhelmingly false positives — `any_instance` from
  RSpec, `mattr_accessor` writers, gem class methods. Narrowing it to
  near-miss evidence (report only when a similar name is actually
  visible) still left 216 new on rails, 12 on discourse and 36 on
  mastodon. A false positive is worse than a miss, so the premise that
  this was an additive, low-risk change is disproven by measurement and
  the gaps stay open.

- An instance-accessor typo on a class whose only class-body call is
  `mattr_accessor`/`cattr_accessor` stays silent, because indexing that
  macro deliberately leaves the class open (see Added, above, and the
  28-site `TopicQuery` measurement that made closing it unacceptable).
  The index does hold the right accessor names; spending them as a
  diagnostic is gated on class openness, which is a separate problem.

## [0.1.0] - 2026-08-25

### Added

- `ita` binary (Rust) with `check` and `server` subcommands: an
  inference-first Ruby type checker in the style of ty/Pyrefly rather than
  Sorbet, parsing via `ruby-prism` and checking with an incremental
  (salsa-based) query engine.
- `ita check`: whole-project type checking with human-readable output, plus
  machine-readable `--format=json` and `--format=agent` modes
  (newline-delimited JSON on stdout, one object per diagnostic with path,
  line, column, code, severity, and message; stable ordering; exit code
  mirrors diagnostic count).
- `ita server`: a real-time LSP server, one process per workspace folder, so
  multiple projects opened in the same editor window never share type
  information. Declaration sources (`db/schema.rb`, `db/structure.sql`,
  Tapioca RBIs under `sorbet/rbi`) are discovered once and shared between
  `check` and `server`. Files outside a server's own workspace root are
  ignored for diagnostics rather than merged into the wrong project.
- Push diagnostics (`textDocument/publishDiagnostics`) and pull diagnostics
  (`textDocument/diagnostic`, LSP 3.17) from the same underlying analysis.
- Go-to-definition: `ita definition <path>:<line>:<col>` on the CLI and
  `textDocument/definition` over LSP, including through `super` calls and
  `define_method` blocks.
- Hover with inferred types: `ita hover <path>:<line>:<col>` on the CLI and
  `textDocument/hover` over LSP, showing a short type and, for resolved
  method calls, the signature and definition site.
- Sorbet interop: inline RBS sig comments (`#: (Integer) -> String`) checked
  against inferred argument and return types, and lazy loading of Tapioca
  RBIs (`sorbet/rbi/`) so gem and core-class ancestry stay accurate without
  a full eager parse.
- Schema-aware checks: `db/schema.rb` and `db/structure.sql` are read as
  declaration sources, so an assignment of a literal to a column whose
  schema type can't hold it is flagged (E0106).
- `ruby-lsp-itaruby`: a ruby-lsp addon gem that registers itaruby as a
  diagnostics formatter, backed by a persistent per-workspace `ita server`
  process it speaks LSP to, so diagnostics arrive on pull instead of
  shelling out to `ita check` per request. Definition and hover are served
  by `ita server` to editors that connect to it directly; routing them
  through ruby-lsp's own listeners is not part of this release.
- A reference VS Code extension (unpublished, local install only) for
  editors that do not run ruby-lsp.
- A zero-false-positive design goal (Invariant #1): when a type cannot be
  proven, itaruby stays silent instead of guessing — an unproven type never
  produces a diagnostic.

[0.1.0]: https://github.com/aryrabelo/itaruby/releases/tag/v0.1.0

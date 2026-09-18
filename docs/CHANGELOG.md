# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
  All four corpora unchanged in both directions: errors byte-identical,
  warnings 923/986/1890 and corpus-c 179 unmoved, singleton residue
  identical at 650/44/4572/1701.

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
- Publishing hygiene for the public launch: workspace package metadata
  (license/repository/homepage) inherited by every crate, the project
  site linked from the README, local scratch paths reworded out of
  comments, and a NOTICE for the third-party diff hunks embedded in the
  bug-replay candidates corpus.

### Fixed

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

# Public-corpus baselines — audit ledger & provenance

These files are the expected-forward state for `scripts/public-gate.sh`'s
drift check against four public Rails apps. They are **generated
measurements, not claims about correctness**.

## ⚠️ The error sets are DRIFT DETECTORS, NOT a claim of truth

Each `<id>.jsonl` is the raw `ita check <clone> --format=json` output
(the `path` field stripped of its clone root and replaced with `<id>/`),
sorted and deduped. The gate fails when the current run's set differs by a
single line — new or gone.

The ledger now records source audits, including the 13 previously unclassified
instances audited on 2026-09-17 against the pinned public clones. AGENTS.md's
corpus rule requires a diagnostic to be proven a true positive by reading the
flagged code before it can be claimed. These 176 error lines (173 at first
generation, +3 audited additions on 2026-09-03) are **not 176 true errors**:
the ledger includes known false positives and an inconclusive external-gem
call. The launch bar ("winning on true errors found") must **not** be claimed
from the raw totals. A wrong diagnostic that enters a baseline stays green
forever; recording a verdict here does not repair the checker or baseline.
As of the **2026-09-19 wave-2 regeneration** the baseline holds
**4 errors = 0 FP + 4 TP** (rails 1, mastodon 0, discourse 3) — every false
positive the ledger had recorded on the public corpora is now closed;
the totals above are the 2026-09-02 history, and the regeneration sections
record what moved — the 160 builder false positives are gone.

Warnings are a separate gap (the external-declaration gap, bead ita-10d);
they are ceilings, not truth claims, and this ledger concerns **errors only**.

## Provenance

Generated 2026-09-02, machine m5 (Apple M5), release binary at commit
ed3a06d (`cargo build --release`, exit 0). Shas pinned from
`git ls-remote <url> HEAD` the same day.

| id | pinned sha | .rb files | errors | warnings | ita wall |
|----|-----------|-----------|-------:|---------:|---------:|
| rails | `3df2cbea2027026a29edb92cbb7e336a63e35444` | 3459 | 167 | 925 | 1.83 s |
| mastodon | `171438e185139efd68df7245f39f57c880dceff6` | 3262 | 0 | 986 | 0.79 s |
| discourse | `dd4cc4f4cc5aa73d8eb8efc3c154f4e139ff6052` | 11837 | 6 | 1890 | 7.67 s |
| gitlab-foss | `4a42053c3a8274a18d5d71fff2f88ba23e5a69e9` | (not cloned) | — | — | — |

gitlab-foss stays absent by design on this machine (multi-GB clone; disk).
Generate its baseline and ceiling on a disk-capable machine.

### Regeneration 2026-09-03 (reason precedence + abstract-raise softening)

All three regenerated on m5 at working tree HEAD `0e64c5e` plus the
uncommitted abstract-raise softening and the open-reason precedence fix
(`AbstractRaise` is the weakest open reason — first-reason-wins previously
let an earlier raise stub swallow a later class-body delegate loop, which
measured 23 false E0101s on discourse's importers and was reverted before
ship). rails and mastodon match their prior baselines byte-for-byte in
content (sort-order flips only). Only discourse gains lines: 6 -> 9
errors, warnings unchanged.

| id | pinned sha | errors | warnings | ita wall |
|----|-----------|-------:|---------:|---------:|
| discourse | `dd4cc4f4cc5aa73d8eb8efc3c154f4e139ff6052` | 9 | 1890 | 8.3 s |

### Regeneration 2026-09-17 (concern `class_methods do` defines `ClassMethods`)

rails only, and only DOWNWARD: 1092 -> 1090 lines, both removed lines the
same false positive —
`E0104 unresolved constant ConcernTest::Baz::ClassMethods` at
`activesupport/test/concern_test.rb:81` and `:87`. The constant really
exists at runtime: `Baz` uses `class_methods do ... end`, and
`ActiveSupport::Concern#class_methods` `const_set`s `ClassMethods` on the
concern. Rails' own passing tests assert exactly that — the same file's
`test_class_methods_are_extended` (`:78-82`) and
`test_class_methods_are_extended_when_prepended` (`:84-88`) both compare
`ConcernTest::Baz::ClassMethods` against the includer's singleton
ancestors. Indexing the block spelling registers the constant, so the two
warnings are gone by correction, not by suppression. mastodon and
discourse are byte-identical. Errors unchanged on all three; this touches
warnings only.

| id | pinned sha | lines | errors | warnings | ita wall |
|----|-----------|------:|-------:|---------:|---------:|
| rails | `3df2cbea2027026a29edb92cbb7e336a63e35444` | 1090 | 167 | 923 | 1.4 s |

### Regeneration 2026-09-18 (literal definers inside a `def` body)

rails only, and only DOWNWARD: 1090 -> 1089 lines, the removed line the
false positive this ledger already recorded as one —
`E0101 undefined method \`_routes\` for \`ActionDispatch::Routing::RouteSet\``
at `actionpack/lib/action_dispatch/routing/route_set.rb:628`. The receiver
there is `helper_module`, the anonymous `Module.new` of `:550` captured at
`:619`, not a RouteSet; the checker was resolving lexical `self`. It goes
silent because `def generate_url_helpers` (`:547`) is an INSTANCE method
whose body carries literal `define_method` calls: `self` is not provably
the class there, so RouteSet now fails closed (open) instead of being
judged against a surface the walker cannot attribute. Gone by correction,
not by suppression.

mastodon and discourse are byte-identical, and the 548f2b8 parent
reproduces all three committed baselines exactly (0 new, 0 gone) — the
measurement is a comparison between two builds in one window, not a
reading of one.

| id | pinned sha | lines | errors | warnings | ita wall |
|----|-----------|------:|-------:|---------:|---------:|
| rails | `3df2cbea2027026a29edb92cbb7e336a63e35444` | 1089 | 166 | 923 | (not timed — measured on a loaded machine) |

### Regeneration 2026-09-19 (re-pin ~2.5 weeks forward)

All three corpora re-pinned to their default-branch HEAD of 2026-09-19 and
every baseline regenerated against the new revision in this commit. Measured
on m5 (Apple M5), release binary at commit `b4acee4` (`cargo build --release`,
exit 0, `CARGO_TARGET_DIR` pinned to the worktree). The canonical clones were
NOT moved — the re-pinned revisions were measured in linked `git worktree`s
(`rails-head`/`mastodon-head`/`discourse-head`), and `scripts/public-gate.sh`
now verifies each declared tree's HEAD against the pinned sha fail-closed, so
a re-pin cannot silently measure the revision it meant to replace.

| id | pinned sha | .rb files | lines | errors | warnings | ita wall |
|----|-----------|----------:|------:|-------:|---------:|---------:|
| rails | `6610cb45b39b6a2c1f260f90bbe920b5899f844b` | 3464 | 1100 | 166 | 934 | 1.5–1.7 s |
| mastodon | `2c92a56e5d0fb490cc2e49a0dd9652499000fcb4` | 3266 | 986 | 0 | 986 | 0.4–0.7 s |
| discourse | `eff621544daf344ce70e470b7f54f27bc75b68d9` | 12280 | 1936 | 7 | 1929 | 5.3–6.6 s |

Ceilings tightened to wall x1.5: rails 3 s (was 3), mastodon 2 s (was 2),
discourse **10 s (was 16)** — the corpus is measurably faster now than the
2026-09-02 anchor, and slack is debt (AGENTS.md).

**Drift vs the 2026-09-02 baselines, classified by `scripts/public-drift-attrib`**
(moved = same `path`+`code`+`message`, only `line` changed — a file grew under
the diagnostic, not a detection change; real = a diagnostic newly emitted or no
longer emitted):

| id | new | gone | moved (line shift) | real new | real gone |
|----|----:|-----:|-------------------:|---------:|----------:|
| rails | 180 | 169 | 168 | 12 | 1 |
| mastodon | 2 | 2 | 2 | 0 | 0 |
| discourse | 364 | 327 | 289 | 75 | 38 |

The raw drift looks large (994 of 1231 total new/gone lines) but is almost all
files moving under the diagnostics. At ERROR severity the real drift is tiny:
**0 new errors, 2 gone errors**, both discourse false positives now silent —
`create_extralite_db` on `ForkedMultiDbWriter` and on `SingleWriter` (the
top-level-helper-through-Object resolution). Everything else in the real-drift
columns is E0104 warnings (the external-declaration gap, ita-10d — ceilings,
not truth claims). rails 166 → 166 errors, discourse 9 → 7, mastodon 0 → 0.

New error totals: **173 = 168 FP + 4 TP + 1 inconclusive** (was 175 = 170 FP +
4 TP + 1 inconclusive; the two removed lines were false positives). The
per-family verdicts in the table below were audited on the 2026-09-02 pins and
are unchanged in content on the new pins — the surviving families' lines only
shifted, verified mechanically by the classifier — so their internal line-refs
are as-of that audit and move a few lines on the re-pin (e.g. `Blog::Post`
`naming_test.rb:328`→`:333`, `BulkImport::Base` `base.rb:1512`→`:1510`).

**gitlab-foss cost (measured 2026-09-19, read-only, NOT adopted — the owner
decides whether it enters):** a `git clone --depth 1` of today's master
(`ec664686`) is ~1.2 GB on disk (working tree ~1.0 GB + `.git` 214 MB) with
55,037 `.rb` files; `ita check` over it ran in **18.4 s** (→ ceiling ~28 s at
x1.5) and emitted 17,545 diagnostics (59 errors, 17,486 warnings, all
unaudited). A `--filter=blob:none` full-history clone was not run (no fresh
clone taken this session); the depth-1 footprint above is the disk budget a
judged gitlab-foss would need. The public declaration and the SKIP stay as
they are.

### Regeneration 2026-09-19 (attributed-mixin family: rails 166 → 6)

All three baselines regenerated at commit `bddae94` (the attributed-mixin
family — the "key on the receiver, never on a name" rule in AGENTS.md) on m5
(Apple M5), release binary built from that commit (`cargo build --release`,
exit 0, `CARGO_TARGET_DIR` pinned to the checkout). Same pinned revisions as
the re-pin above, so this is a detection change measured against a fixed tree,
not a re-pin.

| id | pinned sha | lines | errors | warnings | ita wall |
|----|-----------|------:|-------:|---------:|---------:|
| rails | `6610cb45b39b6a2c1f260f90bbe920b5899f844b` | 940 | 6 | 934 | 1.7 s |
| mastodon | `2c92a56e5d0fb490cc2e49a0dd9652499000fcb4` | 986 | 0 | 986 | 0.7 s |
| discourse | `eff621544daf344ce70e470b7f54f27bc75b68d9` | 1936 | 7 | 1929 | 6.8 s |

rails only, and only DOWNWARD: 1100 → 940 lines, **160 errors gone, 0 new** —
and 0 new on the other two as well, whose files are **byte-identical**, which
is the measurement that the mechanism is keyed on the receiver rather than a
blanket softening of every dynamically-mixed module. All 160 were already
recorded in this ledger as false positives: the two Thor builder families,
`Rails::PluginBuilder` (86) and `Rails::AppBuilder` (74), closed by opening the
class the mixin call's receiver provably names. Both audit rows below are now
marked closed; nothing else in the table moved.

New error totals: **13 = 8 FP + 4 TP + 1 inconclusive** (rails 6, mastodon 0,
discourse 7). The ceilings are unchanged at the re-pin's wall ×1.5 (3 s / 2 s /
10 s): the times above are the gate's own run, the mechanism adds no measurable
wall time, and the wall column is a machine-load reading, not a criterion
median — a tightening from it would be a fabricated measurement.

### Regeneration 2026-09-19 (wave 2: five fail-closed suppressions, rails 6 → 1)

All three baselines regenerated at commit `600bc36` (wave 2's five fail-closed
suppressions: the load-hook installs read by name, the attributed-mixin
receivers, and the three in `check.rs`) on m5 (Apple M5), release binary built
from that commit (`cargo build --release`, exit 0, `CARGO_TARGET_DIR` pinned to
the checkout). Same pinned revisions as the re-pin above, so this is a
detection change measured against a fixed tree, not a re-pin.

| id | pinned sha | lines | errors | warnings | ita wall |
|----|-----------|------:|-------:|---------:|---------:|
| rails | `6610cb45b39b6a2c1f260f90bbe920b5899f844b` | 935 | 1 | 934 | 1.4–2.2 s |
| mastodon | `2c92a56e5d0fb490cc2e49a0dd9652499000fcb4` | 986 | 0 | 986 | 0.4–0.9 s |
| discourse | `eff621544daf344ce70e470b7f54f27bc75b68d9` | 1932 | 3 | 1929 | 6.4–7.9 s |

**Zero new lines on every repo** — the phase removed false positives without
adding a single one, which is the property that matters. The 4 remaining errors
are all true positives already recorded above (rails `route_set_test.rb:18`;
discourse `vbulletin5.rb:77,91` and `base.rb:1510`), so the public corpora now
hold **0 FP**, down from the 8 the ledger had accumulated. All seven audit rows
whose lines this regeneration removes are marked CLOSED above.

**Bead D closed by measurement, not by argument.** `vbulletin3.rb:1339` (E0102,
`postprocess_post_raw`) was D's acceptance criterion — the arity-ambiguity rule.
`git log -S vbulletin3 -- scripts/public-baseline/discourse.jsonl` shows it
entered the baseline only at `0e64c5e` and leaves it here: the sibling
suppressions (C/E/F) closed it, so D has no acceptance left on the pinned
corpora and is dropped exactly as bead A was. The rule it carried stays unbuilt
until a corpus shows the shape.

**The wall numbers were taken twice, and the first pass was noise.** The run
that produced this regeneration measured rails at 3.420 s and discourse at
24.046 s — both over their ceilings — on a machine whose load average was 8.8
and climbing past 22 (the fleet runs several agent sessions here). A same-window
A/B of the two binaries, five alternating runs each, gave rails median
**1.544 s (wave 2) vs 1.606 s (parent `a0492ad`) — 0.96x, no regression** — while
the same binary ranged 1.39–3.67 s across five runs. This is AGENTS.md's rule
("a perf ceiling measured on a loaded machine is not a measurement") demonstrated
on this very gate: the ceilings are wall ×1.5 of a quiet measurement, and this
machine's noise is ×2, so **the public gate's time verdict is flaky under fleet
load** and its red must be re-measured with the parent in the same window before
it is believed. The ceilings are unchanged: re-deriving them from a loaded run
would be the fabricated measurement the re-pin section already refuses.

## Audit ledger (errors)

Every error line is grouped by family — `code` and the receiver class taken
from the message (`E0101`/`E0104`: `for \`X\``), or the method name for
`E0102`. Verdicts below are source-reading conclusions, not runtime verification.
The 2026-09-17 audit classified 13 previously blank instances: **1 true positive,
11 false positives/correct-silence cases, 1 inconclusive**. Two of the 11 are
intentional negative tests: their exceptions are real and expected, not bugs.
Paths in each row are relative to that row's pinned public repository. The baseline
JSONLs were read only; no checker, build, test, or dependency installation was run;
the existing 160 builder-FP and 3 bulk-import-TP verdicts are unchanged.

| id | family (code + receiver) | count | verdict |
|----|---------------------------|------:|---------|
| rails | E0101 `Rails::PluginBuilder` | 0 (was 86) | **known FP family — Thor builder `method_missing` (docs/engineering-history.md §H), not a true positive**; CLOSED 2026-09-19 by the attributed-mixin family — the receiver the `include` names is opened, so the whole family is silent (0 new errors on the three corpora) |
| rails | E0101 `Rails::AppBuilder` | 0 (was 74) | **known FP family — Thor builder `method_missing` (docs/engineering-history.md §H), not a true positive**; CLOSED 2026-09-19 by the attributed-mixin family, same mechanism as `Rails::PluginBuilder` above |
| rails | E0101 `LazyLoadHooksTest::FakeContext` | 0 (was 3) | **CLOSED 2026-09-19 by wave 2's fail-closed suppressions** — the load-hook installs are read by name, so the three sites are silent; false positive / correct silence — `activesupport/test/lazy_load_hooks_test.rb:131-137` installs `first_wrestler` via the class hook; `:160-174` installs `second_wrestler` only on `context` (valid at `:174`, intentionally absent on the fresh instance inside `assert_raises NoMethodError` at `:168-170`); `activesupport/lib/active_support/lazy_load_hooks.rb:101-110` dispatches through `class_eval` / `instance_eval`. |
| rails | E0101 `ActionDispatch::Routing::RouteSet` | 0 (was 1) | false positive, FIXED 2026-09-18 and removed from the baseline — `actionpack/lib/action_dispatch/routing/route_set.rb:628` calls `helper_module._routes`, not a RouteSet instance: `Module.new` at `:550`, `helper_module = self` at `:619`, singleton `_routes` at `:599`, and proxy reader at `:561-564` supply the method and return the captured route set. The enclosing `def generate_url_helpers` (`:547`) is an instance method carrying literal `define_method` calls, so RouteSet now fails closed instead of being judged. |
| rails | E0101 `ActionDispatch::Routing::Endpoint` | 0 (was 1) | **CLOSED 2026-09-19 by wave 2's fail-closed suppressions** — false positive — `actionpack/lib/action_dispatch/routing/endpoint.rb:11-15` defaults `rack_app` to the endpoint, but `rack_app.is_a?(Class) && rack_app < Rails::Engine` short-circuits for that instance; the comparison is for a class-valued rack app, using inherited `Module#<`, not `Endpoint#<`. |
| rails | E0101 `ActionDispatch::Routing::RouteSetTest::SimpleApp` | 1 | true positive (dormant test helper) — `actionpack/test/dispatch/routing/route_set_test.rb:12-20` defines an Object subclass with no mixins/accessor: initialization stores `@response` at `:14`, but `call` bare-sends `response` at `:18`; dispatching this Rack app would raise `NoMethodError`. Uses at `:26-34,55-72` register routes/check helpers rather than dispatching, so those assertions do not exercise the defect. |
| rails | E0101 `Blog::Post` | 0 (was 1) | **CLOSED 2026-09-19 by wave 2's fail-closed suppressions** — false positive — the instance send at `activemodel/test/cases/naming_test.rb:328` is provided by `activemodel/test/models/blog_post.rb:8-9` extending `ActiveModel::Naming`; its `extended` hook at `activemodel/lib/active_model/naming.rb:263-266` delegates instance `model_name` to the class, whose method is at `:280`. |
| mastodon | (no errors) | 0 | — |
| discourse | E0101 `ForkedMultiDbWriter` | 0 (was 1) | false positive, GONE from the 2026-09-19 baseline — no longer emitted at `eff62154` (the top-level `create_extralite_db(path, initialize: false)` at `migrations/tooling/scripts/benchmarks/write.rb:27-36` now resolves through Object's private instance methods for the `:107` subclass), so removed by correction, not suppression. |
| discourse | E0101 `Migrations::Conversion::Base` | 0 (was 1) | **CLOSED 2026-09-19 by wave 2's fail-closed suppressions** — false positive / correct silence — `migrations/core/lib/migrations/conversion/base.rb:13-16` calls `setup` only inside `if respond_to?(:setup)`; the base has no setup and skips it, as does the concrete subclass at `migrations/converters/lib/migrations/converters/discourse/converter.rb:6-12`; this is an optional hook, not an unguarded missing-method call. |
| discourse | E0101 `SingleWriter` | 0 (was 1) | false positive, GONE from the 2026-09-19 baseline — no longer emitted at `eff62154` (same top-level-helper resolution as `ForkedMultiDbWriter`; the `:54` Object subclass reaches `:27-36` with no receiver), so removed by correction, not suppression. |
| discourse | E0102 method `postprocess_post_raw` | 0 (was 1) | **CLOSED 2026-09-19 by wave 2 — this was bead D's acceptance criterion, closed by its sibling beads (C/E/F), which is why D is dropped by measurement rather than built** — false positive — `script/import_scripts/vbulletin3.rb:1339` supplies two arguments to its own two-argument definition at `:1563`; that script requires `base` at `:4` and defines `ImportScripts::VBulletin` at `:20`. The separate v4 entrypoint `script/import_scripts/vbulletin.rb:21,884` reuses the class name with a one-argument method, but is not loaded by the v3 entrypoint; merging their signatures invents the arity error. |
| discourse | E0102 method `can_edit_tag?` | 0 (was 1) | **CLOSED 2026-09-19 by wave 2's fail-closed suppressions** — false positive / correct silence (intentional negative test) — `spec/lib/guardian/tag_guardian_spec.rb:98-100` deliberately omits the required tag inside `expect { ... }.to raise_error(ArgumentError)`; `lib/guardian.rb:34` includes `TagGuardian`, whose `lib/guardian/tag_guardian.rb:18` requires one argument. The exception is the asserted behavior, not a defect. |
| discourse | E0102 method `body` | 0 (was 1) | **CLOSED 2026-09-19 by wave 2's fail-closed suppressions** — inconclusive (receiver-blind suppression candidate) — `lib/email/message_builder.rb:200-203` sends `body html` inside `Mail::Part.new`, while the enclosing builder's unrelated `body` at `:206` takes zero arguments. Missing evidence: the pinned mail gem's `Mail::Part`/`Mail::Message` constructor block-evaluation behavior and body setter arity; these dependency sources are not present in either audited clone, so source-only inspection cannot prove the DSL receiver/arity. |
| discourse | E0101 `BulkImport::VBulletin5` | 2 | true positive — proven by reading `script/bulk_import/vbulletin5.rb:77,91`; receiver `BulkImport::VBulletin5` has no `import_user_account_id`, no `create_oauth_records` (ancestry checked: `BulkImport::Base` — abstract-raise stub at `base.rb:409`, no later stronger open reason — then Object; no `def`/`attr_*`/`alias`/literal `define_method`/`method_missing`/`delegate`/`Forwardable` for either name anywhere in the repo) |
| discourse | E0101 `BulkImport::Base` | 1 | true positive — proven by reading `script/bulk_import/base.rb:1512`; `process_user_stat` bare-sends `user_email` with no local of that name in scope, and receiver `BulkImport::Base` has no `user_email` (ancestry checked: no written superclass → Object, no mixins; no `def`/`attr_*`/`alias`/literal `define_method`/`method_missing` repo-wide) |
## Dark-census residue audit — 2026-09-20 (bead ita-tail follow-up)

The singleton census's post-tail residue (56/4/44 closed_notfound on
rails/mastodon/discourse, commit `d9a95f5`) was audited family by family:
51 families, 104 records, every representative site read at its byte
offset. Verdicts:

**True positives found in the wild (2 confirmed + 1 pending, ~6 sites):**

| family | verdict | evidence |
|---|---|---|
| `ColorMath::Converters#RuntimeError` (discourse, 4) | **TRUE POSITIVE — real bug in discourse** | `lib/color_math.rb:62`: `raise new RuntimeError("Hex color must be 6 characters")` — parsed as `raise(new(RuntimeError("…")))`; `RuntimeError` is a constant, not a method, so the validation branch raises NoMethodError instead of the intended message. Author meant `RuntimeError.new(...)`. |
| `RaisesNoMethodError#foobar_method_doesnt_exist` (rails, 1) | TRUE POSITIVE (by design) | `activesupport/test/autoloading_fixtures/raises_no_method_error.rb` — the fixture exists to raise NoMethodError. |
| `DiscourseAi::Completions::Llm#models_by_provider` (discourse, 1) | **SUPERSEDED — re-audited 2026-09-21 at the flip.** Previously classified as "census noise (instance-context call)" but re-examined as true positive: `llm.rb:79` sits inside `def valid_provider_models`, a SINGLETON method (class `<< self` block spans lines 20-150), so the bare call is on the class object. No `def self.models_by_provider` exists tree-wide; this is a real dead singleton method. |
| `DiscourseAi::Utils::DiffUtils#apply_hunk` (discourse, 3) | **SUPERSEDED — re-audited 2026-09-21 at the flip.** Previously classified as "census noise (instance-context call)" but re-examined as true positive: `ai_artifact.rb:72-74` receives a LOCAL `differ = DiscourseAi::Utils::DiffUtils` (a module object), so the call is on the class-object track and conclusive. No `def self.apply_hunk` on the module; this is dead code. |
| `DiscourseAi::Agents::General#id` (discourse, 1) | likely TRUE POSITIVE — re-verify at flip | `bot_controller.rb:136` `DiscourseAi::Agents::General.id`; no `def self.id` found tree-wide. Chain closed per census. |

**Populated — every one must stay silent; each names its bead (98 sites):**

| mechanism | families/sites | unlocking bead |
|---|---|---|
| class/def nested in method bodies/blocks: the walker stubs them WITHOUT their superclass (`class Foo < Rails::Railtie` inside a test method) — config/railtie_name/instance/attributes_for_inspect/model_name/fetch_data | ~20 | walker: process block-nested class/def defs (or mark their stubs open, fail-closed) |
| ActiveSupport core-ext on Class/Module/Object/Kernel: `descendants`, `module_parent(s)`, `module_parent_name`, `in?`, `silence_warnings` | 23 | lock-gated AS core-ext name inventory (mock-gate pattern) |
| `extend ActionView::Helpers::TextHelper` (declared-external module extended): `TextHelper#excerpt/truncate` | 12 | extend-into-declared-external ⇒ Inconclusive |
| gem-internal singletons with unmodeled `.with`: ExceptionWrapper/ExecutionContext/JSON::Encoding/SchemaReflection | 9 | same extend/ancestor-external blocker once stubs are fixed |
| `include Singleton` (+ ancestor `Rails::Railtie`): `Subscriber#instance`, `Foo#instance` | 7 | Singleton-instance softener, include-visible-gated |
| sclass-include mixin: `class << self; include Redisable` ⇒ bare `redis` in class methods | 4 | sclass-include filing on the singleton track |
| eval-built class surface: `AnonymousCache.__compiled_key_builder` built by `eval(method)` inside a method body | 5 | opaque-eval mark must fire for eval inside method bodies, not only class body |
| gem-namespace receiver: `QC#default_conn_adapter(=)` (queue_classic) | 4 | gem-namespace receiver ⇒ Inconclusive |
| `extend self` module surface: `Marshal70/71WithFallback#marshal_load` | 2 | extend-self filing |
| census attribution noise (wrong file/line/byte on duplicated records; `define_method`/`Class.new` block bodies): `send_shortcut=`, `description=`, duplicate variants of every `ok` record | ~10 + dupes | instrument bead: dedupe records by (file, byte), fix span/triple corruption, exclude non-lexical-self block bodies from the class-object walk |

**Verdict: the class-object E0101 flip is NOT shippable today.** 98 of 104
records are populated silence; flipping now would fire ~98 false positives —
the exact outcome the audit exists to prevent (invariant #1). The nine beads
above are each small, mechanical, and two-sided-provable; after them the
residue is the true-positive table above, and the flip converts the two
confirmed discourse/rails sites into diagnostics while the bench's two gap
rows (`extend_singleton_typo`, `included_hook_class_method_typo`) flip to
hits.

## Regeneration 2026-09-21 — the class-object E0101 flip

The class-object track's conclusive `MethodLookup::NotFound` became a
diagnostic (`crates/itaruby_semantic/src/check.rs`, the `Ty::Class` arm),
gated on `inconclusive_reason(c, true) == None` — the exact predicate the
dark census had been bucketing as `closed_notfound` while the arm was
silent. Twelve mechanisms landed first, each a NAMED open reason or an
indexed surface keyed on the receiver's own chain, and the census residue
| bead | mechanism | records closed (rails/mastodon/discourse) |
|---|---|---|
| ita-xta | `extend M` walks M's OWN ANCESTRY (`include`/`prepend`), and an OPEN ancestor there makes the surface unreadable rather than empty | 1 / 0 / 2 |

### NEW lines — 8 total; 7 true positives, 1 by-design fixture

**Rails:** 1 error (the `RaisesNoMethodError#foobar_method_doesnt_exist` fixture at `activesupport/test/autoloading_fixtures/raises_no_method_error.rb:4`, designed to raise `NoMethodError` — not a discovered defect).

**Discourse:** 7 errors — every one a true positive, read at its byte offset and proven to raise.

Tree-wide greps below are over the pinned `-head` clone, `--include=*.rb`,
for `def <name>` / `def self.<name>` / `attr_*` / `alias` / `alias_method`
/ `define_method(:<name>` / `define_singleton_method(:<name>` / `delegate`
/ `method_missing` on the receiver and every ancestor of its chain.

Per-mechanism, measured on the pinned `-head` clones:

| bead | mechanism | records closed (rails/mastodon/discourse) |
|---|---|---|
| ita-xta | `extend M` walks M's OWN ANCESTRY (`include`/`prepend`), and an OPEN ancestor there makes the surface unreadable rather than empty | 1 / 0 / 2 |
| ita-obx | the class-object chain continues `Class -> Module -> Object -> Kernel -> BasicObject`, so project reopenings of `Object`/`Kernel` (activesupport's `core_ext`) are read | 10 / 0 / 0 |
| ita-sgl | `include Singleton` installs `instance`/`_load`/`clone` on the includer's class object (receiver-keyed on the resolved ancestor `Singleton`, name-keyed on the three) | 5 / 0 / 0 |
| ita-blk | a `class`/`module` KEYWORD inside a class-body block is registered at its LEXICAL path, born open | 4 / 0 / 0 |
| ita-src | a BARE STUB whose name the project also writes as `class X` inside a string literal is opened | 1 / 0 / 0 |
| ita-qcn | `queue_classic` -> `QC` in `gem_namespace`'s override table (read out of the gem archive at 4.0.0) | 4 / 0 / 0 |
| ita-bgd | `BigDecimal` joins the bundled-gem Kernel table (`--disable-gems` cannot harvest it) | 4 / 0 / 0 |
| ita-dsm | `base.define_singleton_method(:x)` in a `self.extended(base)` hook installs on the extender's class object | 0 / 0 / 4 |
| ita-esc | a `self.extended(base)` hook whose `base` escapes the shallow walk (a nested block) is OPAQUE | 0 / 0 / (same 4) |
| ita-evb | a bare `eval(<string>)` in a METHOD body opens the enclosing class | 0 / 0 / 1 |
| ita-slf | an explicit `self` receiver inside a rebindable block is as unprovable as a receiverless one | 0 / 0 / 2 |
| ita-scl | `class << self; include M` files M on the SINGLETON surface | 0 / 1 / 0 |

### NEW lines — 8, every one a true positive proven by reading

Tree-wide greps below are over the pinned `-head` clone, `--include=*.rb`,
for `def <name>` / `def self.<name>` / `attr_*` / `alias` / `alias_method`
/ `define_method(:<name>` / `define_singleton_method(:<name>` / `delegate`
/ `method_missing` on the receiver and every ancestor of its chain.

| id | line | proof |
|---|---|---|
| rails | `activesupport/test/autoloading_fixtures/raises_no_method_error.rb:4` `undefined method \`foobar_method_doesnt_exist\` for class \`RaisesNoMethodError\`` | TRUE POSITIVE BY DESIGN — `class RaisesNoMethodError` (:3) has no superclass and no mixins; the fixture exists to raise `NoMethodError` when autoloaded. Tree-wide `foobar_method_doesnt_exist` -> only this line. |
| discourse | `lib/color_math.rb:62` `undefined method \`RuntimeError\` for class \`ColorMath::Converters\`` | TRUE POSITIVE, real bug — `raise new RuntimeError("Hex color must be 6 characters")` parses as `raise(new(RuntimeError("…")))`; arguments evaluate first, so `RuntimeError(...)` raises `NoMethodError` and the intended message never appears. `module ColorMath::Converters` (:35) is a single definition with no `extend`/`include`/`method_missing`; tree-wide `def (self\.)?RuntimeError` -> 0 hits. Author meant `RuntimeError.new(...)`. |
| discourse | `plugins/discourse-ai/app/controllers/discourse_ai/ai_bot/bot_controller.rb:137` `undefined method \`id\` for class \`DiscourseAi::Agents::General\`` | TRUE POSITIVE — `class General < Agent` (`lib/agents/general.rb:5`) defines only instance methods; `class Agent`'s `class << self` block spans `lib/agents/agent.rb:8-292` and contains NO `id`; the only `def id` is at :294, OUTSIDE the sclass, i.e. an instance method. No `extend`/`include`/`prepend`/`method_missing`/`alias` in `agent.rb` and no reopening of `Agent`/`General` anywhere; tree-wide `def self\.id\b\|define_singleton_method(:id\|define_method(:id` -> 0 hits. Reached on the fallback path when the post/topic carries no agent id or name. |
| discourse | `plugins/discourse-ai/app/models/ai_artifact.rb:72` `undefined method \`apply_hunk\` for class \`DiscourseAi::Utils::DiffUtils\`` | TRUE POSITIVE, dead code — `differ = DiscourseAi::Utils::DiffUtils` then `differ.apply_hunk(...)`; the receiver is a module object held in a local, so the class-object track is the right track. `module DiffUtils` is a pure namespace opened in three files (`lib/utils/diff_utils/{hunk_diff,simple_diff,safety_checker}.rb`) containing only the classes `HunkDiff`, `SimpleDiff`, `SafetyChecker`; no `def` on the module itself, no `extend`/`include`/`method_missing`/`define_method`/`delegate`. Tree-wide `apply_hunk` -> only these three calls plus a spec `describe ".apply_hunk"` whose subject calls `described_class.apply`. The real API is `DiffUtils::HunkDiff.apply` (`hunk_diff.rb:85`); `apply_diff` itself has zero callers. |
| discourse | `plugins/discourse-ai/app/models/ai_artifact.rb:73` (same) | same |
| discourse | `plugins/discourse-ai/app/models/ai_artifact.rb:74` (same) | same |
| discourse | `plugins/discourse-ai/lib/completions/llm.rb:79` `undefined method \`models_by_provider\` for class \`DiscourseAi::Completions::Llm\`` | TRUE POSITIVE, dead singleton method — `def valid_provider_models` (:75) sits inside `class << self` (:20-150), so the bare `models_by_provider` is a class-object call. Tree-wide `models_by_provider` -> only this line, no definition anywhere; no `extend`/`include`/`method_missing` in `llm.rb`. `valid_provider_models` has zero callers. |
| discourse | `plugins/discourse-subscriptions/app/serializers/discourse_subscriptions/payment_serializer.rb:33` `undefined method \`find\` for class \`DiscourseSubscriptions::User\`` | TRUE POSITIVE, masked by `rescue` — inside `module DiscourseSubscriptions`, `User` resolves lexically to `DiscourseSubscriptions::User`, a NAMESPACE MODULE (`app/controllers/discourse_subscriptions/user/{payments,subscriptions}_controller.rb:4 module User`), not `::User`. The plugin is a Rails Engine, so Zeitwerk sets that autoload and `Module#find` does not exist. Tree-wide: no `def self.find`, no `extend`, no `include`, no `method_missing` on it. MRI really raises; the `rescue StandardError -> nil` two lines down swallows it, which is why `PaymentSerializer#username` is always nil. |

### GONE lines — 30, every one a FALSE POSITIVE the wave repaired

All 30 are rails `E0104 unresolved constant` and all 30 are bead **ita-blk**:
the constant was defined by a `class`/`module` KEYWORD inside a class-body
block (`test "..." do class CallMeMaybe ... end end`), which the walker never
descended into, so the reference to it resolved to nothing. Registering the
definition at its lexical path makes the reference resolve. Representative
sites, one per file:

| file:line | constant |
|---|---|
| `actioncable/test/server/socket_test.rb:137` | `CallMeMaybe` |
| `actionmailer/test/base_test.rb:315,327,339,349,368,896,911,926,941,972,989` | `LateAttachmentMailer`, `LateInlineAttachmentMailer`, `LateInlineAttachmentAccessorMailer`, `LateInlineAttachmentMailer`, `LateAttachmentAccessorMailer`, `BeforeActionMailer`, `AfterActionMailer`, `DefaultInlineAttachmentMailer`, `FooMailer`, `DefaultFromMailer`, `MailerWithCallback` |
| `activejob/test/cases/serializers_test.rb:92,93` | `DummySerializerAlt` |
| `activerecord/lib/active_record/signed_id.rb:29` | `DeprecateSignedIdVerifierSecret` |
| `activesupport/test/autoloading_fixtures/raises_name_error.rb:4` | `FooBarBaz` |
| `activesupport/test/core_ext/module/concerning_test.rb:104,115` | `Foo::ClassMethods` |
| `railties/test/application/configuration_test.rb:3549` | `DummyDestroyAssociationAsyncJob` |
| `railties/test/application/url_generation_test.rb:37` | `MyApp` |
| `railties/test/command/base_test.rb:27,40,48,55,71,76,94,110,113` | `Rails::Command::{Hidden,Helpful,Nesting::Nested,CustomBin,LastSubcommand}Command` |
| `railties/test/railties/railtie_test.rb:37` | `FooBarBaz` |

No GONE line is a detection this wave lost: every one is a reference the
checker could not resolve because the walker had not read the definition.

## Regeneration 2026-09-22 — conflicting-superclass reconciliation (main's 1341582)

rails and mastodon regenerated, and only DOWNWARD. discourse is byte-identical
and was not touched. Release binary built from task/sig-rbi at `864125d` (the keyword-walk
fix; source byte-identical to the tree the binary was built from)
(sha256 prefix `8f92e643ff1aa6f7`). Run against the pinned `-head` clones and
normalized exactly the way `scripts/public-gate.sh` does it: the clone root in
`path` becomes `<id>/`, only lines starting with `{` are kept, then `sort -u`.

| id | pinned sha | lines before → after | errors | warnings before → after |
|----|-----------|---------------------:|-------:|------------------------:|
| rails | `6610cb45b39b6a2c1f260f90bbe920b5899f844b` | 906 → 873 | 2 → 2 | 904 → 871 |
| mastodon | `2c92a56e5d0fb490cc2e49a0dd9652499000fcb4` | 986 → 973 | 0 → 0 | 986 → 973 |
| discourse | `eff621544daf344ce70e470b7f54f27bc75b68d9` | 1939 → 1939 (unchanged) | 10 → 10 | 1929 → 1929 |

0 NEW lines. Error counts did not change.

**Where the drift came from.** This drift is on main, not on this branch. We
bisected with scratch release builds. `3f7592a` (the previous re-pin),
`218e5b7` and `4981963` reproduce the committed baselines. `1341582` ("fix:
keep the cbase marker on a harvested alias target") removes exactly these 46
lines. It reached main through the task/alpha-ci merge (`6d04dc7`, PR #2)
without a re-pin, so origin/main `05a282e` already measured rails 33 / mastodon
13 gone. The mechanism is `reconcile_superclasses` / `ambiguous_ancestry` in
`index.rs`. Before it, the first `< Base` header the index saw won. Now a class
declared with incompatible superclasses in different files, or a class whose
ancestry reaches such a class, is ambiguous. An inherited constant lookup
inside it cannot conclude, so E0104 stays silent. This is a fail-closed
suppression, not a new resolution. Every constant it silenced below really
exists at runtime.

**The 85252d8 keyword regression was fixed, not re-pinned.** On this branch,
`85252d8` briefly removed 4 more lines: mastodon `Webpush` at
`app/workers/web/push_notification_worker.rb:53,54` and discourse
`Logster::Web` at `config/routes.rb:51,55`. The new keyword-argument loop in
`check.rs` stopped walking hash elements that are not symbol-keyed: string
keys, constant keys and `**splat` values. Those elements fell into
`infer_expr`'s `_ => Ty::Unknown` arm, which does not visit their children, so
every diagnostic inside them was lost. A scratch fixture showed it: an E0101
error on `take('str' => Widget.nonexistent_class_method)` and E0104 on
`take('k' => Missing)`, `take(Missing => 1)` and `take(**Missing)` all went
silent. Those 4 lines were false positives (the `webpush` and `logster` gems
are in Gemfile.lock), but the change that removed them was a blind spot, not a
correction. The loop now walks a non-symbol pair's key and value and a splat's
value, and all 4 lines are present again in the baselines above.

### GONE lines — 46, every one a FALSE POSITIVE going away

All are `warning E0104 unresolved constant`. Each ambiguous class is named with
its conflicting headers.

| id | file:line | constant | verdict |
|----|-----------|----------|---------|
| rails | `activesupport/test/cache/stores/mem_cache_store_test.rb:11,22,23,26,45,46,49,385,432,436`; `actionpack/test/dispatch/session/mem_cache_store_test.rb:42,43,185` (13) | `Dalli`, `Dalli::Client`, `Dalli::DalliError`, `Dalli::VERSION`, `Dalli::Protocol`, `Dalli::Protocol::Meta`, `Dalli::Protocol::Binary` | FALSE POSITIVE. Both files `require "dalli"`, and dalli 5.0.6 is in Gemfile.lock. `MemCacheStoreTest` is declared `< ActiveSupport::TestCase` and `< ActionDispatch::IntegrationTest`. `Dalli::Protocol::Binary` (:26) is only in the `else` of `if Dalli::VERSION >= "5."`, a compatibility branch for older dalli that the locked version never runs. |
| rails | `activejob/test/cases/logging_test.rb:22,401,406,425,430,436,441,447,452,471,476,495,500` (13) | `ActiveSupport::Logger::Severity`, `WARN`, `INFO`, `FATAL`, `ERROR` | FALSE POSITIVE. `ActiveSupport::Logger < ::Logger` (`activesupport/lib/active_support/logger.rb:8`), and stdlib `::Logger::Severity` defines the levels, which the class includes at :22. `LoggingTest` is declared `< ActiveSupport::TestCase` and `< ActionController::TestCase` (`actionpack/test/controller/logging_test.rb:5`). |
| rails | `activerecord/test/cases/dirty_test.rb:13`, `attribute_methods_test.rb:21` (2) | `InTimeZone` | FALSE POSITIVE. It is `ActiveRecord::TestCase::InTimeZone` (`activerecord/test/cases/helper.rb:38`), inherited. `DirtyTest` and `AttributeMethodsTest` are also declared `< ActiveModel::TestCase` in activemodel. |
| rails | `activerecord/test/cases/adapters/abstract_mysql_adapter/connection_test.rb:11` (1) | `SQLSubscriber` | FALSE POSITIVE. It is `ActiveRecord::TestCase::SQLSubscriber` (`helper.rb:21`), inherited through `ActiveRecord::AbstractMysqlTestCase`. `ConnectionTest` is also declared `< ActionCable::Connection::TestCase`. |
| rails | `activerecord/test/cases/inheritance_test.rb:559,560` (2) | `Firm::FirmOnTheFly` | FALSE POSITIVE. `Firm.const_set :FirmOnTheFly, Class.new(Firm)` at :555 creates it first. `Company` is declared `< AbstractCompany` and `< ActiveRecord::Base` (actionpack/actionview fixtures), which makes `Firm < Company` ambiguous through its ancestry. |
| rails | `activerecord/test/cases/validations_test.rb:136` (1) | `IncorporealModel` | FALSE POSITIVE. `Object.const_set :IncorporealModel, ...` at :133. `ValidationsTest` is also declared `< ActiveModel::TestCase`. |
| rails | `guides/bug_report_templates/action_controller.rb:41` (1) | `Rack::Test::Methods` | FALSE POSITIVE. The file does `require "rack/test"`. `BugTest` has four different superclasses across `guides/bug_report_templates/*.rb`. |
| mastodon | `lib/paperclip/vips_lazy_thumbnail.rb:4,26,27,38,65,72,73,74,75,94,100`; `lib/paperclip/lazy_thumbnail.rb:4,17` (13) | `Paperclip::Processor`, `Paperclip::Thumbnail`, `Geometry`, `TempfileFactory`, `Terrapin::CommandLine`, `Terrapin::ExitStatusError`, `Terrapin::CommandNotFoundError`, `Paperclip::Error`, `Paperclip::Errors::CommandNotFoundError`, `Vips::Image` | FALSE POSITIVE. These come from gems locked in Gemfile.lock: kt-paperclip 8.0.0, terrapin 1.1.1 and ruby-vips 2.3.0. The bare `Geometry`/`TempfileFactory` resolve lexically to `Paperclip::Geometry`/`Paperclip::TempfileFactory`. `Paperclip::LazyThumbnail` is declared `< Paperclip::Processor` and `< Paperclip::Thumbnail` in the two files. |

No GONE line is a lost detection. Every one is a reference to a constant that
exists at runtime, either from a locked gem, inherited through the test base,
or created by `const_set` just before the reference.

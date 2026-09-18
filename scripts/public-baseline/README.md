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
| rails | E0101 `Rails::PluginBuilder` | 86 | **known FP family — Thor builder `method_missing` (docs/engineering-history.md §H), not a true positive** |
| rails | E0101 `Rails::AppBuilder` | 74 | **known FP family — Thor builder `method_missing` (docs/engineering-history.md §H), not a true positive** |
| rails | E0101 `LazyLoadHooksTest::FakeContext` | 3 | false positive / correct silence — `activesupport/test/lazy_load_hooks_test.rb:131-137` installs `first_wrestler` via the class hook; `:160-174` installs `second_wrestler` only on `context` (valid at `:174`, intentionally absent on the fresh instance inside `assert_raises NoMethodError` at `:168-170`); `activesupport/lib/active_support/lazy_load_hooks.rb:101-110` dispatches through `class_eval` / `instance_eval`. |
| rails | E0101 `ActionDispatch::Routing::RouteSet` | 1 | false positive — `actionpack/lib/action_dispatch/routing/route_set.rb:628` calls `helper_module._routes`, not a RouteSet instance: `Module.new` at `:550`, `helper_module = self` at `:619`, singleton `_routes` at `:599`, and proxy reader at `:561-564` supply the method and return the captured route set. |
| rails | E0101 `ActionDispatch::Routing::Endpoint` | 1 | false positive — `actionpack/lib/action_dispatch/routing/endpoint.rb:11-15` defaults `rack_app` to the endpoint, but `rack_app.is_a?(Class) && rack_app < Rails::Engine` short-circuits for that instance; the comparison is for a class-valued rack app, using inherited `Module#<`, not `Endpoint#<`. |
| rails | E0101 `ActionDispatch::Routing::RouteSetTest::SimpleApp` | 1 | true positive (dormant test helper) — `actionpack/test/dispatch/routing/route_set_test.rb:12-20` defines an Object subclass with no mixins/accessor: initialization stores `@response` at `:14`, but `call` bare-sends `response` at `:18`; dispatching this Rack app would raise `NoMethodError`. Uses at `:26-34,55-72` register routes/check helpers rather than dispatching, so those assertions do not exercise the defect. |
| rails | E0101 `Blog::Post` | 1 | false positive — the instance send at `activemodel/test/cases/naming_test.rb:328` is provided by `activemodel/test/models/blog_post.rb:8-9` extending `ActiveModel::Naming`; its `extended` hook at `activemodel/lib/active_model/naming.rb:263-266` delegates instance `model_name` to the class, whose method is at `:280`. |
| mastodon | (no errors) | 0 | — |
| discourse | E0101 `ForkedMultiDbWriter` | 1 | false positive — `migrations/tooling/scripts/benchmarks/write.rb:114` is a bare call to the top-level `create_extralite_db(path, initialize: false)` defined at `:27-36`; the class at `:107` inherits Object, where Ruby installs top-level methods as private instance methods, so this call resolves. |
| discourse | E0101 `Migrations::Conversion::Base` | 1 | false positive / correct silence — `migrations/core/lib/migrations/conversion/base.rb:13-16` calls `setup` only inside `if respond_to?(:setup)`; the base has no setup and skips it, as does the concrete subclass at `migrations/converters/lib/migrations/converters/discourse/converter.rb:6-12`; this is an optional hook, not an unguarded missing-method call. |
| discourse | E0101 `SingleWriter` | 1 | false positive — `migrations/tooling/scripts/benchmarks/write.rb:58` resolves the top-level helper at `:27-36` through Object's private instance methods; the Object subclass at `:54` needs no explicit include or receiver for the call. |
| discourse | E0102 method `postprocess_post_raw` | 1 | false positive — `script/import_scripts/vbulletin3.rb:1339` supplies two arguments to its own two-argument definition at `:1563`; that script requires `base` at `:4` and defines `ImportScripts::VBulletin` at `:20`. The separate v4 entrypoint `script/import_scripts/vbulletin.rb:21,884` reuses the class name with a one-argument method, but is not loaded by the v3 entrypoint; merging their signatures invents the arity error. |
| discourse | E0102 method `can_edit_tag?` | 1 | false positive / correct silence (intentional negative test) — `spec/lib/guardian/tag_guardian_spec.rb:98-100` deliberately omits the required tag inside `expect { ... }.to raise_error(ArgumentError)`; `lib/guardian.rb:34` includes `TagGuardian`, whose `lib/guardian/tag_guardian.rb:18` requires one argument. The exception is the asserted behavior, not a defect. |
| discourse | E0102 method `body` | 1 | inconclusive (receiver-blind suppression candidate) — `lib/email/message_builder.rb:200-203` sends `body html` inside `Mail::Part.new`, while the enclosing builder's unrelated `body` at `:206` takes zero arguments. Missing evidence: the pinned mail gem's `Mail::Part`/`Mail::Message` constructor block-evaluation behavior and body setter arity; these dependency sources are not present in either audited clone, so source-only inspection cannot prove the DSL receiver/arity. |
| discourse | E0101 `BulkImport::VBulletin5` | 2 | true positive — proven by reading `script/bulk_import/vbulletin5.rb:77,91`; receiver `BulkImport::VBulletin5` has no `import_user_account_id`, no `create_oauth_records` (ancestry checked: `BulkImport::Base` — abstract-raise stub at `base.rb:409`, no later stronger open reason — then Object; no `def`/`attr_*`/`alias`/literal `define_method`/`method_missing`/`delegate`/`Forwardable` for either name anywhere in the repo) |
| discourse | E0101 `BulkImport::Base` | 1 | true positive — proven by reading `script/bulk_import/base.rb:1512`; `process_user_stat` bare-sends `user_email` with no local of that name in scope, and receiver `BulkImport::Base` has no `user_email` (ancestry checked: no written superclass → Object, no mixins; no `def`/`attr_*`/`alias`/literal `define_method`/`method_missing` repo-wide) |
# Engineering history — itaruby

This is the accumulated per-bead engineering log of itaruby, ported from the
archived alpha repo before it closed. It records the reasoning and the
measured numbers behind each fix, wave by wave — a lesson without its
measurement is folklore, not knowledge. The binding contracts this history
produced live in the root [`AGENTS.md`](../AGENTS.md), under Global Contracts
and Rules of proof, never here; this file is narrative record, not the
contract surface.

## Engineering History — waves 1 through 10 (ported from the archived alpha repo)

This section is the accumulated per-bead engineering log of the project, carried
over from the private archive repo before it closed. Every client name has been
replaced by its corpus id (`corpus-a` = the first work-machine corpus, `corpus-b`
= the second work-machine corpus, `corpus-c` = the strong-machine corpus — the
same ids `scripts/corpus-baseline.txt` uses) and every symbol that exists only in
one client's code has been replaced by a description of its shape. Numbers are
kept verbatim: a lesson without its measurement is folklore, not knowledge.

### A. Editor & protocol integration

- **Shared discovery + ruby-lsp bridge** (waves 1-2): `wire_declaration_sources`
  in `discovery.rs` is the single owner of the upward search for `db/schema.rb`
  > `db/structure.sql` and `sorbet/rbi`; both `ita check` and `ita server` call
  it. **Root containment:** a `didOpen` outside the server's root is ignored
  entirely (no `SourceFile`, no diagnostic) plus a deduped WARNING log — before
  this fix, two projects opened in the same editor window silently merged into
  one ancestry index and could fabricate E0101-E0104 that neither project would
  produce alone (a false positive, invariant #1 breach). Path comparison is
  `canonicalize` on both sides with a lexical-by-component fallback (the macOS
  `/tmp`->`/private/tmp` trap), never substring. `ita check --format=json`:
  JSONL on stdout, one object per diagnostic, 1-based, same order as the human
  render; `--format=agent` is byte-identical. The addon gem
  (`scripts/ruby-lsp-itaruby/`) implements the ruby-lsp addon's 3-method
  interface; the rule that almost bit: NEVER filter by exit code (`ita` exits 1
  whenever there ARE diagnostics) — binary failure or bad output becomes an
  empty array plus a log line, never an exception in the host.
- **Sig fill, pull diagnostics, persistent runner** (`method_return`, wave 2):
  falls back to a method's own `sig { returns(X) }` only when the inferred body
  type is Unknown; an inferred type never loses to a sig. (Superseded
  2026-09-22: the sig is now a contract that types the consumer, and a body
  that disagrees is accused as E0109. See the contract in the root
  `AGENTS.md`.) The **`sig{}`
  carve-out** — a recognized sig block (returns/void, with
  params/override/abstract) stops marking its class `open` — unmasked 63
  diagnostics on the reference corpus, all read: 3 precision fixes (a reopened
  gem class treated as a complete view; Kernel/Object private methods missing
  from the core allowlist; self-send inside a block/lambda is never conclusive
  E0101, because `instance_exec` rebinds self) plus 1 true positive (a bare
  call on a rescue path with no reader — a dormant `NameError`). Cost accepted:
  one baseline true positive (self-send inside an iteration block) went
  inconclusive. Baseline after this bead: corpus-c expects 3 errors, corpus-a
  expects 2. Pull diagnostics (`textDocument/diagnostic`, LSP 3.17) shares the
  exact publish-path query, zero duplicated conversion. The persistent runner
  keeps one `ita server` per workspace (LSP framing over stdio, mutex, 10s
  select timeout, 1 respawn then disable). Measured on corpus-c: blind
  84.9% -> 80.8%.
- **Self-send conclusivity inside blocks that don't rebind self**
  (`core_block_keeps_lexical_self`): an ALLOWLIST, never a denylist, of methods
  known to just `yield` — an unlisted name is merely unproven, costing a false
  negative rather than a false positive. Gated only for known core receiver
  classes; project/Union/Unknown receivers stay silent even for
  protocol-standard iterator names, because a project's own `each` is exactly
  where a hand-rolled rebinding iterator lives. A more aggressive,
  name-only variant on `Ty::Unknown` receivers was **reverted in review** even
  though it measured exactly one new true-positive diagnostic across 5866 files
  in the three corpora, zero collateral: a method NAME is an argument about a
  published contract, never proof of who runs it, and invariant #1 forbids
  gating a diagnostic on argument rather than proof. A follow-up dead end
  (treating the pattern as its own diagnostic code) was killed before being
  built once measurement showed the safe form would never fire on any corpus
  and the aggressive form is a false positive by construction. Six mutants, all
  CAUGHT, including a **regression lock**: the fallback path mutated to call
  the allowlist function again, letting the wide `Unknown` arm sneak back
  behind its own guard.
- **Constraint-based inference + `--format=agent`** (TypeProf-style,
  deterministic, **no embedded model** — an earlier `ita infer` design with its
  own model was cut and must never be resurrected there): every
  `Ty::Unknown`-receiver call on a local-variable read becomes a "responds to
  m" constraint; at method-scope end, methods (>=2, dedupe first-wins)
  intersect against closed-ancestry project classes union 8 concrete core
  classes (`Object`/`Kernel` excluded on purpose — every receiver responds to
  `.class`/`.freeze`, which would fabricate a false contradiction). Empty
  intersection -> E0107 (Warning, never Error, never touches exit code), gated
  on `closed_world()` being ON (only `ita check`, never the LSP) and every
  method in the set having >=1 candidate. Hardened by a blind A/B versus
  TypeProf 0.30.1 across three rounds with a fresh critic each round, SHIP on
  round 3. Design decisions kept even though they cost nothing measurable: FP
  suppressions are never narrated (silence is the honest output of invariant
  #1); a per-file "clean" line was rejected as noise on 2000-file corpora in
  favor of a single run-level footer.
- **Go-to-definition**: `definition_at` reuses the checker's own
  `MethodLookup` resolution in silent mode, zero duplicated logic. Two clients:
  `ita definition <path>:<line>:<col>` and `textDocument/definition`.
- **`include`/`extend`/`prepend` with a dynamic receiver**: the `DefWalker` had
  a generic early return — any class-body call with an explicit receiver was
  ignored before name dispatch — so a `T.unsafe(self)`-wrapped dynamic include
  never opened the class: ancestry stayed closed by mistake and a real mixin
  method became a false-positive E0101. Fixed by dispatching by name before
  that early return, opening the class whenever the receiver isn't implicit or
  explicit `self`.
- **VS Code client** lives in `scripts/vscode/`, not `editors/` at the repo
  root — that path trips the software-factory root-files rule (measured
  failing before this decision). Missing binary shows an explicit error
  message naming what's missing, never silent — same navigation invariant as
  everywhere else: no-answer is acceptable, a silently wrong or omitted answer
  never is.
- **`super` and `define_method(:x) do...end` in go-to-definition**:
  `super_lookup` continues MRO linearization from the ancestor NEXT AFTER the
  one that physically defines the current method, never from the start; for a
  method defined only inside a mixed-in module, the lookup scans every project
  class that actually mixes the module and continues from that class's own MRO
  slot — if two consuming classes would answer differently, or any relevant
  chain is incomplete, it's silence, never a guess (invariant #1 extended to
  `super`). `define_method` with a literal name now indexes even inside a
  block — previously any class-body call with a block marked the class `open`
  unconditionally, making the dedicated `define_method` handling unreachable in
  its common written form.
- **LSP polish (hover + cancellation)**: hover reuses `definition_at`'s silent
  walk; `Ty::Unknown` renders `null`, never an invented type (invariant #1
  extended to hover). Under rapid `didChange`, a request queue plus cancel set
  drains all pending edits before answering any request, so a stale-revision
  answer can never leak; `$/cancelRequest` before dispatch returns
  RequestCancelled.

### B. Type-inference depth

- **Curated public-gem declarations** (`declarations/gems.rbi`): E0104 was
  dominated by a handful of never-locally-defined public-gem namespaces — one
  sorbet-runtime constant alone was ~60% of corpus-c's 17231 warnings at the
  time. Only names a real measurement showed dominating got declared, each as
  an empty module/class with zero methods (`merge_declared_fragment` forces
  `open = true` unconditionally on a declared entry — it resolves the constant
  and nothing else, ancestry never closes, because we don't know whether the
  real gem has `method_missing`). Only fills a path the project never defined
  itself; a real project reopen always wins. Three candidate constants
  (occurring 1050/1004/515 times on corpus-c) were investigated and
  **excluded**: they never resolved anywhere inside the app in any corpus —
  they were an app-level alias defined in a client initializer (a common
  gem-shortening convention), not a name the gem itself exposes; declaring them
  would have been the same app-symbol lookup table the anti-gaming rule
  forbids, disguised as a declaration file. Measured on corpus-c: warning
  ceiling 17231 -> 5661, error hash set unchanged.
- **Ivar typing + `is_a?`/`nil?` narrowing** (wave 3): narrowing covers only
  local variables and parameters — ivars are excluded on purpose, since any
  method can mutate an ivar at any time. `if x.is_a?(Foo)` types the `then`
  branch `Instance(Foo)`; `x.nil?`/`unless x.nil?` types the nil branch
  `Ty::Nil` and strips `Nil` from an already-known type on the other branch —
  it never invents a concrete type from `Unknown` (invariant #1 by
  construction). Ivar typing walks every instance method once per class
  (never singleton), and deliberately does **not** use `Ty::Union`: the
  argument-compatibility check treats a `Union` argument as "every member must
  match", stricter than `Unknown`'s "always passes", so a real union here could
  still manufacture a false-positive E0103; the same-type-twice-keeps,
  different-type-or-any-Unknown-collapses-to-Unknown fold was used instead.
  Measured on corpus-c: error hashes and the 5661 warning ceiling both
  unchanged — zero new diagnostics. **Fixture-naming lesson:** `ita check
  testdata/` scans the whole tree as one project, so a class name reused
  across fixture directories merges ancestry and leaks a diagnostic from one
  fixture into another (found here when two shared names collided and produced
  a false E0101 in a file meant to stay silent) — every fixture directory
  since uses a globally-unique class-name prefix.
- **`ivar_ty` 15x perf regression fix**: the ivar-typing bead above re-walked
  *all* of a class's instance methods from scratch per distinct ivar name —
  O(distinct ivars x methods); on the reference corpus's backend `app/models`
  directory this took a directory from 1.61s to 24.33s. Fixed by capturing
  every ivar write in one pass per class, keyed per-name, with the recursion
  guard moved from `(class, ivar name)` to `class` alone (a nested read of a
  *different* ivar on the same class mid-walk now falls back to `Ty::Unknown`
  instead of triggering its own independent re-walk — safe under invariant #1,
  verified byte-identical on the corpus). Measured: `app/models` 24.33s ->
  1.38s (target <2.5s), whole `app` 1.76s (target <4s); no test edited, all 97
  pass.
- **E0101 conclusive on core receivers under a closed world** (`ClosedWorld`
  salsa input, wired only into `ita check`, never `ita server`/LSP — byte
  identical to the pre-wave behavior there). Fires only when (a) no
  Gemfile/lock/gemspec is discovered within 4 levels, OR a Gemfile with
  COMPLETE Tapioca coverage exists (one gem missing its RBI fails the whole
  mode closed to silence — measured 14/292 gems missing an RBI on corpus-c at
  the time -> mode off, baseline intact by construction; PATH/GIT gems are
  never covered); (b) the core class isn't reopened by the project nor
  dynamically injected; (c) the method is outside both the hand allowlist and a
  mechanically-harvested core-method inventory (regenerated from a clean Ruby
  install; the guard aborts below Ruby 3.0 because an older, smaller inventory
  would false-positive) and, under Tapioca mode, outside every declared core
  reopening collected from every RBI (first-wins would drop the second gem
  reopening a core class and false-positive). Blind A/B versus Sorbet on 8
  synthetic probes: itaruby TP=5 FP=0 FN=0 at 0.003s; Sorbet TP=4 FP=1 (flags a
  correct dynamic include) at 0.121s.
- **Wave 3 — require/autoload**: two capabilities, both measurement-gated —
  candidates that measured near-zero gain on the reference corpus were **not**
  built. (1) stdlib constants gated by require: a stdlib constant only
  silences E0104 if some project file has the matching `require` — silence
  only, FP impossible by construction. (2) external ancestry via gem RBI in
  constant lookup: a BFS over gem RBIs (superclass + include/prepend, cap 48,
  memoized). Measured on corpus-c: warning ceiling 4321 -> 2298, errors
  unchanged, wall time 1.67s -> 1.45s.
- **RBI-based method resolution — the biggest single lever measured in the
  project.** Three decisions diverged from the original proposal, all by
  measurement: (1) **no Tapioca-coverage gate** — the capability is
  additive-only (`Inconclusive -> Found` never emits a diagnostic;
  `Inconclusive -> NotFound`, where a false E0101 would be born, doesn't exist
  on this path — proved with a dedicated regression lock, not assumed). (2)
  **no sorbet-sig-to-`Ty` mapping** — results type to `Ty::Unknown`, zero arity
  checking, so this shipped version *measures* the lever's size without
  *cashing* it; normal `ita check` cost was unchanged. (3) **BFS cap raised 48
  -> 4096**, plus edge-type set-crossing: a real activerecord RBI declares its
  base class with **123 direct edges** (63 include + 60 extend) — the
  inherited cap-48 (sized for a 3-link chain) truncated before reaching it;
  `extend`/the Concern class-methods marker move a module's INSTANCE methods
  into the receiver's SINGLETON set (how a class-level query method exists via
  `extend`) — the order between the two sets is load-bearing. Measured on
  corpus-c: the new bucket covered 6773 call sites (4.7%), blind 96.7% ->
  92.0% — the single largest drop of the session. **Strategic finding:**
  neither corpus-a nor corpus-b ships an RBI tree at all — 61.5%/60.1% of
  their respective ancestry-open buckets are unreachable by this path until
  the client itself runs a static-typing generator; the project's biggest
  lever is gated on client tooling, not on our code.
- **Sorbet-sig return-type pipeline, and the measurement that redirected the
  whole bead.** Arity stayed out by design decision: a stale sig relative to
  the installed gem would false-positive an arity error; the mapping can only
  ever render a core type, a core collection type, or `Unknown` — core
  receivers still only accuse conclusively under the closed-world gate (off on
  all three corpora). **Result: 3 call sites moved.** Root cause, measured, and
  it kills the bead's premise: an RBI generated from a gem is a reflection of
  runtime, and reflection infers no type. Sig-per-definition coverage across
  ~1900 real RBI files broke out as roughly 104% for app-model DSL RBIs versus
  **6.1%** for plain gem RBIs — and the dominant ancestor's own gem RBI file
  measured **zero** sig coverage. The 88%+ aggregate coverage figure was
  entirely the app's own Tapioca-generated model RBIs (typed accessors for the
  app's models), not gem ancestry at all. **Lesson, third time it paid in this
  line of work: measure the SOURCE before building the consumer** — without
  the 6.1%-versus-104% number, this bead's report would read "capability
  shipped, small gain, sig lever is small" — the wrong conclusion, because the
  sig lever is enormous, just not where the bead was looking.
- **DSL RBI as a type source for the app's own models** (additive-only): a
  method lookup against the class's own and its PROJECT ancestors' Tapioca DSL
  RBIs, with a discovered precedence rule — an unqualified nested edge inside
  an already-loaded DSL RBI resolves inside that file first, because the
  global RBI-name map collides across ~1587 DSL files (last-wins otherwise,
  silently attributing a type to the wrong class). Measured on corpus-c: blind
  92.0% -> 84.9% (the type propagates down the call chain, `project ret`
  21053 -> 15237), diagnostics unchanged, ceiling intact; the other two
  corpora were byte-identical (no RBI tree to read).

### C. Schema-as-declaration

- **`db/schema.rb`/`db/structure.sql` as type declarations**: synthesizes a
  reader+writer per column on classes whose literal superclass is an
  ActiveRecord base; never overwrites a method the model itself defines.
  Unmapped column types (decimal/date/datetime/json/jsonb/inet) resolve to
  `Ty::Unknown` — safe under invariant #1, a false negative rather than a
  false positive. The cast-mismatch warning fires only on a literal String RHS
  that's non-numeric for a numeric column, or has no digit for a
  date/datetime column — any other form (variable, call, interpolation) stays
  silent, because the framework's own cast is too permissive to risk more.
  **Binding rule (2026-08-20): schema found by upward search is a declaration
  source, never code under review** — indexes, never diagnoses (a corpus's own
  `db/schema.rb` had rendered a false-positive constant warning and moved the
  ceiling before this rule existed). The `db/structure.sql` variant (needed
  because one corpus is a pure Postgres dump with no `schema.rb`) is a
  hand-written parser scoped only to `CREATE TABLE`/column, producing the same
  internal index — `schema.rb` always wins the precedence race when both
  exist.

### D. Coverage census & attribution methodology

- **`ita check --stats` coverage census**: five buckets (project method / core
  method / diagnosed / ancestry open / receiver unknown), `blind = open +
  unknown`, gated so re-walks from other inference passes never inflate the
  count (proved by mutation: without the gate, the total doubles). Census
  snapshot: corpus-a 83.0% blind, corpus-b 87.9%, corpus-c 96.9% (the only
  corpus where ancestry-open dominates, consistent with its gem-namespace
  E0104 count at the time). Strategic read: receiver-unknown is the common
  floor (~46-51% across all three) -> inference depth is the structural
  ceiling there; ancestry-open is the slice attackable by declarations.
- **Origin of receiver-unknown**, 8 sub-buckets summing to the parent bucket
  by construction:

  |sub-bucket|corpus-a|corpus-b|corpus-c|
  |---|---|---|---|
  |method param|4.8%|4.0%|5.2%|
  |block param|2.3%|4.0%|5.6%|
  |core ret|0.3%|0.6%|0.2%|
  |project ret|16.6%|12.7%|15.4%|
  |dead chain|13.0%|16.8%|15.6%|
  |ivar|4.8%|7.3%|0.7%|
  |constant|2.7%|4.0%|1.6%|
  |other|1.2%|1.1%|1.9%|
  |**receiver unknown (total)**|45.7%|50.5%|46.3%|

  This measurement killed one proposed lever outright: core-return-via-RBS's
  entire addressable surface is `core ret`, 0.2-0.6%, 20-50x smaller than any
  other bucket. Left standing, conditionally: parameter inference, attacking
  `method param` + `block param` (7-11%) — but see the next entry before
  building anything against it.
- **The `project ret` cause was measured, and the leading hypothesis was
  false.** By call site (never by method definition — a helper called 400
  times weighs 400x a single-use one):

  |cause of `project ret`|corpus-a|corpus-b|corpus-c|
  |---|---|---|---|
  |param|1|0|4|
  |ivar|1|0|0|
  |constant|1|0|0|
  |explicit return|14|0|0|
  |other (assignment ceiling)|2816 (26.1%)|196 (18.9%)|532 (2.4%)|
  |**callee unresolved**|**7973 (73.8%)**|**842 (81.1%)**|**21842 (97.6%)**|

  `project ret` is 74-98% unresolved callee: the same ancestry-open problem
  propagating one link forward, not a return-inference gap. Parameter is 0-4
  call sites out of 10-22 thousand — the parameter-inference bead's own gate
  condition ("only build if parameter dominates the bucket") was not met by
  the measurement.
- **Ancestry-open attribution by blocker, and the first closure.** Every
  ancestry-open site records a reason, first-reason-wins, with a counterfactual
  precedence (an unresolved name outranks a declared-but-methodless constant,
  which outranks a project-side reason):

  |blocker|corpus-a|corpus-b|corpus-c|
  |---|---|---|---|
  |unresolved name|8861 (36.2%)|818 (26.8%)|33551 (45.7%)|
  |declared open|6527 (26.7%)|1050 (34.4%)|29516 (40.2%)|
  |project dsl|5856 (23.9%)|555 (18.2%)|1018 (1.4%)|
  |project block|563 (2.3%)|54 (1.8%)|7481 (10.2%)|
  |project meta|945 (3.9%)|178 (5.8%)|34 (0.0%)|
  |project missing|195 (0.8%)|0|240 (0.3%)|
  |not ancestry|1528 (6.2%)|396 (13.0%)|1553 (2.1%)|

  A bucket folding two mechanisms together hid the fix twice in this one bead:
  splitting a mismatched pair out of a catch-all bucket revealed a
  `included do...end`-style idiom worth 7481 call sites on corpus-c against
  1018 for the real catch-all — combined, the read would have been "model the
  framework's whole DSL", the wrong fix, 7.3x smaller than real; and an
  `external` bucket at 61-86% split almost 50/50 between "name doesn't
  resolve" and "name resolves but method doesn't attach". **First closure:** an
  allowlist of class-body calls proven to register a hook/validator and define
  nothing (public framework/job/queue-library API only, never an app symbol) —
  every method GENERATOR stays excluded, because allowlisting a generator
  would close a class that must stay open and fabricate a false E0101.
  Measured: errors unchanged, ceilings intact; blind fell corpus-a 83.3% ->
  82.0% (-835 sites), corpus-b 87.9% -> 86.8% (-84), corpus-c 96.9% -> 96.7%
  (-376). **Lesson (5th time this line of work paid it): the unit of
  measurement decides the conclusion.**

### E. Precision fixes

- **Constant lookup follows the ancestor chain**: root cause was that constant
  existence checks only walked lexical scope; real Ruby order is lexical
  nesting THEN the innermost scope's ancestors. The type-producing constant
  resolver stays lexical-only ON PURPOSE — widening it would trade
  `Ty::Unknown` for a real class type and could fabricate new diagnostics
  (invariant #1); the existence-only check is monotonically safe to widen,
  since it only suppresses a false E0104. Measured on corpus-a: warnings 560
  -> 532, error set identical; corpus-b unchanged.
- **Constant visibility — two real false-positive fixes plus one non-finding
  worth as much.** Defect A: a constant written inside a class-body-call block
  was never indexed, because any class-body call with a block (other than a
  couple of recognized forms) marked the class `open` and returned without
  recursing — fixed with a one-level recursion that harvests literal constant
  writes for the LEXICALLY ENCLOSING class, safe because constant definition is
  lexical (`instance_eval`-family calls rebind `self`, never the enclosing
  scope). Defect B: a top-level constant assignment was invisible to
  resolution, fixed as a terminal step after the existing bare-name widening
  loop. Measured:

  |corpus|warnings|errors|
  |---|---|---|
  |corpus-c|2298 -> **1525** (-773, -33.6%)|3, hashes identical|
  |corpus-b|103 -> **92** (-11)|0|
  |corpus-a|603 -> **603** (zero change)|2, hashes identical|

  **The non-finding:** a batch of "up to 11" candidate false positives on
  corpus-a turned out to be a measurement-method ghost — the bead was born
  from a naive text scan matching class/module declarations at line-start
  without considering nesting, so a nested class counted under its bare name
  instead of its qualified name. Redone with a real parser and qualified-name
  matching, all 11 dissolved (root-name collision with a public gem the app
  also reopens as an unrelated internal namespace, or a genuinely dead
  reference correctly flagged) — all 603 corpus-a warnings are correct.
  **Lesson (5th time this line of work paid it): the unit of measurement
  decides the conclusion** — root-segment and qualified-name matching answer
  different questions, and the former fabricated an entire false-positive bead
  that didn't exist.
- **Self-send resolves against descendants, not only ancestors**: a self-send
  dispatches on the RUNTIME class, so an abstract base class whose template
  method calls hooks only its sole instantiated subclass defines resolved fine
  in production but the checker declared it `NotFound` — a false positive
  found by an EXTERNAL reviewer, not internally: 3 of 6 "verified" baseline
  errors on corpus-a were exactly this form. Fixed with a subclass map built
  once at the end of project indexing (deliberately not scanned per miss,
  because this query runs in E0101's hot path); only converts `NotFound` to
  `Inconclusive`, never the reverse, so it's monotonically safe. `super`'s own
  lookup deliberately did **not** get the same rule — `super` dispatches
  upward explicitly, so applying it there would change `super`'s semantics
  rather than fix a false positive. Measured: corpus-a errors 6 -> 3 (the 3
  removed hashes are exactly the 3 false positives, zero new hash); corpus-b
  unchanged.
- **Deterministic diagnostic order**: the sort lives in the CLI's print loop,
  never in the vector that feeds index construction — reordering that vector
  would be a semantic change disguised as a render fix.

### F. Repo hygiene, honesty fixes, root validation

- **Three honesty fixes, one round, zero diagnostics changed**: (1) the
  workspace's one remaining compiler warning was a dead match arm, deleted.
  (2) A doc comment claiming gem-generated RBI files never nest constants was
  false for one class of generated file — an indented header passed through a
  trim and entered a lookup map under a bare, unqualified name, and ~1587
  generated files reused the same names, so the map's key collided and the
  winner was arbitrary filesystem order. The surviving contract, now written
  down where the next builder will read it first: **that map is a coarse file
  locator, never resolved-name ground truth — every consumer must refilter
  against the real qualified path.** A collision degrades to a silent miss,
  never to a wrong type. (3) An asymmetry in the block-rebind guard got a name
  and a documented reason instead of staying implicit folklore.
- **A nonexistent root is a usage error, not a clean run**: walking a
  never-existing path yielded zero files, zero diagnostics, and exit 0 — byte
  identical to "your code is clean." In CI, a typo'd path or wrong working
  directory let a job pass having checked nothing. Fixed by validating each
  root's existence before the walk and exiting with a distinct usage-error
  code, naming the path — a directory that exists with no Ruby files in it
  still exits 0 and silent, because checking a not-yet-Ruby subtree is
  legitimate. Later hardened to distinguish "definitely not there" from
  "couldn't tell" (a root behind a permission-denied parent, or a vanished
  mount, isn't a typo, and reporting it as "not found" sends the operator
  hunting the wrong bug).
- **Public-repo hygiene audit** (zero diagnostic-semantics lines touched,
  `ita check testdata/ --format=json` byte-identical before/after): (1) new
  files can't land at the repo root at all under this repo's own root-file
  policy, so CHANGELOG/CONTRIBUTING/CODE_OF_CONDUCT moved to their
  GitHub-recognized subdirectory locations; `LICENSE` is the real exception
  (package registries and the GitHub UI only detect it at root) and stayed
  human-gated. (2) The CI hazards job (deny-warnings clippy) could never pass
  — the workspace silently carried 58 clippy warnings, unnoticed because the
  job never once ran green; zeroed with zero new suppressions and an unchanged
  test count. **A clippy suggestion was refused with a reason**: the literal
  suggested rewrite would have silently changed a documented
  skip-and-continue behavior into an abort-on-first-error — solved with an
  equivalent rewrite instead. **Lesson: a lint that effectively asks for a
  behavior change gets the equivalent rewrite, never the literal suggestion.**
  (3) CI showing red on every push turned out to be a billing gate, not a code
  problem — no code fix addresses that, and the real evidence is always the
  local gate script's own transcript. A third-party tool install in CI got a
  commit pinned, so an upstream push couldn't silently change this repo's CI
  result. (4) A client-library version ceiling had to be kept byte-identical
  between the gemspec and the addon's own runtime guard — divergence is
  exactly the bug this rule exists to prevent, because the addon API is
  declared experimental upstream. (5) The LSP client gem was hardened at three
  points that had been silently degrading: percent-encoding a workspace root
  URI per path segment (space/accent in the workspace path previously killed
  the addon silently), a bounded shutdown handshake before escalating to a
  hard kill, and a write-side timeout symmetric to the existing read-side one
  (a full pipe was blocking a change notification inside a mutex and hanging
  the host editor entirely). (6) The LSP server crate went from zero tests of
  its own to three, covering root containment on both branches and URI
  encoding. A large single-file split was measured and refused: the repo's own
  lint policy has no file-size rule, only a per-function complexity ceiling,
  and refactoring without a measured violation is speculation.

### G. The symlink incident

**Reaching the project root through a symlink fabricated 310 false positives —
the single largest invariant-#1 violation ever measured in the project, found
while anchoring an unrelated launch bead, no dedicated ticket.** The upward
search for a Gemfile/lock/gemspec was lexical: through a symlink, the lexical
parents are the LINK's parents, so a Gemfile sitting next to the real app
directory became invisible — and that absence wasn't neutral, because the
closed-world gate reads "no Gemfile within 4 levels" as "this project has no
gems", which is the license to flag core-class calls as nonexistent. Measured
on the identical corpus, sha, and file content: through the real root, **2
errors / 603 warnings** (the sacred baseline); through a symbolic root, **310
errors**, every one the same family of ActiveSupport core extensions.
**"Absence of evidence was being read as evidence of absence, in the one code
path that turns silence into an error" (learned 2026-08-24, binding).** Fixed
by making one function the sole source of upward-walk candidate directories,
resolved via canonicalization, where an unresolvable result means "couldn't
tell" and is treated as "gems present" (fails closed to silence) rather than
"not there." Only the walk resolves the symlink — the path printed in a
diagnostic stays exactly what the user passed, or every corpus baseline would
drift at once. **The real lesson from this bead:** the first fix version
returned the fully canonicalized path, and every consumer downstream does
membership tests against paths it already has in some other form (an open
editor buffer's path, a file-discovery list) — returning a canonical form
where nothing else is canonical simply stops matching, silently exempting an
open buffer from diagnostics it should receive. Fixed by yielding, per search
level, the caller's own path form first and the resolved form only as
fallback. **"A test whose result depends on whether the machine's tempdir
happens to pass through a symlink is luck with a name" (learned 2026-08-24,
binding)** — the regression had only failed on one of two development
machines; the paired test now builds the symlink on disk so it decides the
same everywhere. Separately, **the corpus gate had gone mute about all of this
because the corpus itself had moved on disk** — the declared path stopped
existing (the corpus moved down one directory level), so the gate SKIPped it,
and a skip exits with the identical code to "this machine legitimately doesn't
host that corpus" — no push since had actually re-proven that corpus. **"A
SKIP on a declared corpus, on a machine that should host it, means the path
rotted — not that the machine lacks it" (learned 2026-08-24, binding, already
a rule above — this is its origin story).** The temptation that must never be
repeated: restoring a moved corpus with an on-disk symlink "works" and is
exactly what produced the 310 false positives — a corpus that moves gets fixed
in its declaration, never patched with a filesystem link.

### H. Launch bar & the public-corpus precision campaign

- **Rails false positives and the fail-closed decision**: the first run on a
  large public framework codebase (3455 files) produced 3131 errors, and an
  audit proved at least 95.8% of them false positive, root-caused to three
  mechanisms (core/stdlib/gem class reopens treated as closed project classes;
  a generic `.new` call typed with default-constructor arity; a
  namespace-nesting bug in constant resolution). **User decision, binding:
  where itá can't prove, it stays silent and the gap becomes roadmap — never a
  diagnostic** (already a rule above; this is its origin story). Measured:
  3131 -> **305 errors** (-90.3%), the two dominant false-positive families at
  zero, zero drift on all three private corpora. Left out with a documented
  ceiling and a follow-up ticket: a structural nesting mechanism (6 sites,
  later fixed — see below) and a metaprogramming-heavy builder-object idiom
  from a popular CLI-framework gem (194 sites, deliberately abandoned — see
  below). The next round found 6 more false-positive mechanisms outside that
  framework, all shipped: an RBS comment-syntax parser too narrow for
  documented forms; constant aliasing; a gem-RBI mixin not propagating through
  the ancestor chain; a few missing core-class declarations; `...` argument
  forwarding read as zero-arity; and `.new` on an initializer-less class
  defaulting to a zero-arg constructor.
- **Head-to-head against Sorbet+Tapioca on a strict, Sorbet-typed public repo**:
  Sorbet found 0 errors in a fraction of a second after a much slower
  `bundle install`; itaruby's own diagnostics on the same repo sampled 38
  items read line by line, 38/38 false positive — all six mechanisms above.
  Honest published conclusion at the time: on that terrain, then, Sorbet won.
- **Public-corpus scale ladder**: four major open-source Rails/Ruby codebases
  checked end to end, timings ranging roughly 4-41 seconds single-threaded
  depending on size, determinism proved byte-identical after sorting. The
  fail-closed fixes above *accelerated* every run measurably (less wrong
  conclusive lookup, less cascade) even though they existed purely for
  precision.
- **Launch bar, user decision, binding: ship only when itá beats Sorbet on all
  four public reference repos on BOTH speed and true errors found.** Every
  error Sorbet finds and proves real that itá misses becomes a launch-blocking
  false-negative ticket — "we have to be better than Sorbet; nobody switches
  for something worse." First reciprocal audit: 240 candidate diagnostics
  examined by hand across the four repos, **zero true positives Sorbet has
  that itá misses** — every one traced to a missing gem RBI, framework
  metaprogramming, or a limitation of Sorbet's own flow analysis. Speed was
  still lost on 3 of 4 repos (single-threaded), making multithreading
  launch-blocking.
- **Launch-gate integration round** (three isolated workstreams plus lead
  integration, all corpora re-proved on the final sha, zero drift): (1)
  **multithreading by default**, using the underlying query engine's own
  canonical parallel idiom with zero plumbing changes — determinism proved
  across every output format and all four public repos, byte-identical. One
  pre-existing did-you-mean tie-break bug surfaced and got fixed along the way
  (hash-map iteration in rendering is only safe with the tiebreak key
  included). Measured: the four-repo union check time fell from over two
  minutes to under 40 seconds; speed now ties Sorbet 2-2 on the launch-bar
  repos. (2) **A full RBS comment-signature parser rewrite** covering every
  documented syntax form, measured eliminating essentially all false-positive
  malformed-comment warnings on two reference repos. (3) **The curated parser
  unmasked a real type-compatibility bug** — exact-equality checking on
  instance types was false-positiving genuine subtype calls; fixed to check
  ancestry membership with incomplete ancestry silencing (invariant #1). (4)
  **Real lexical-nesting-stack scoping** replaced a flat-string approximation,
  closing the framework nesting gap left open by the Rails fail-closed round.
  (5) **One proposed fix was ABANDONED WITH MEASUREMENT** — refusing an unsafe
  fix is also a deliverable: a global heuristic silenced 100% of one corpus's
  real errors, and a name-based heuristic created a real, measured collateral
  false negative (an innocent class matching the heuristic's naming pattern
  opened and killed a true positive three files downstream); the underlying
  metaprogramming idiom's false positives remain a documented, un-closed
  ceiling rather than an oversight.
- **Two more precision rounds on strict, heavily-typed public repos**: gem-RBI
  mixin resolution was fixed to walk the full ancestor chain instead of just
  the receiving class's own path (measured collapsing one repo's false
  positives from over a thousand to about 20, against a target of 50 or
  fewer); guard-based and `case`/`when`-based type narrowing were extended
  (truthy checks, `nil?` combined with `||`, class-based `when` clauses under
  a soundness restriction that every clause condition must be a resolvable
  constant) — one overly broad first version of this self-detected via a
  corpus diff during development and was narrowed before shipping.
- **Two further rounds closed the remaining false-positive mechanisms found by
  the reciprocal public audit**: declaring a couple more core-language
  constructs; following simple constant aliasing without touching the
  type-producing resolver; refusing to default a reopened gem class to a
  zero-arg constructor; fixing `...` forwarding arity extraction; declaring a
  few more gem-style Kernel-mixin methods and test-framework mixin methods;
  reopening classes named in the lockfile; a structural fallback for
  never-locally-declared namespaces; and a literal-only `self.included` hook
  handler. Each of these five to nine independent fixes was measured
  individually and shipped only with its own mutation proof, re-run personally
  by the lead in every round. **Two operational incidents from this period,
  registered so they aren't repeated:** (learned, binding) isolated background
  work that the lead cannot cherry-pick by its commit sha does not exist — on
  any "merge failed" notice, the first action is hunting the sha/branch
  immediately, before anything else, because a detailed report does not save
  work that lives only in a disposable, already-deleted clone. Second, smaller:
  two same-wave workstreams editing the same function in the same file
  produced a real merge conflict resolved by hand — same-wave work touching
  the same file needs an explicit disjoint-region note in the brief, and the
  lead still reviews merge order regardless.

### I. The ActiveRecord-API and cache-rejection saga

- **The ablation that decided everything after it**: neither corpus-a nor
  corpus-b ships a static-typing RBI tree at all, so the RBI-resolution lever
  above measures zero on both by construction — confirmed first, not
  inherited as an assumption. Ancestry-open was measured at **36.4% of call
  sites on both of those corpora, identical to the decimal** (registered as
  coincidence, not law). Removing one declaration and re-measuring (not
  estimating) showed the base ActiveRecord-style class alone accounts for
  **77-79%** of the declared-open bucket on those two corpora.
- **On-disk caching was measured and rejected, twice, and must not be
  reopened** (already a rule above — this is the fuller origin story): parsing
  a whole gem's source fresh measured a median of about 25 milliseconds
  against a roughly 1.8-second corpus-check baseline (1.4%); validating a
  cache by a correct key (name + version + content hash) measured **more
  expensive than reparsing** (33-58ms); no corpus's lockfile carries a
  checksums section, so the key wouldn't even be free to compute. Separately,
  indexing the real installed gem source as an alternative to RBI-only lookup
  was measured and rejected as an isolated lever: (1) it still yields
  "inconclusive" for the dominant call pattern, because most of the relevant
  ancestor chain opens via a class-body-block idiom regardless of source; (2)
  the gem literally didn't exist on disk on either host machine for two of the
  three corpora (both run containerized, the gem only exists inside an image
  layer); (3) the real source declares FEWER edges than the generated RBI for
  the same class (roughly 38% as many), and part of the gap is unreachable by
  any static parser at all (a dynamically constructed include target). **The
  finding that mattered:** the overwhelming majority of a base class's open
  ancestors open for exactly one reason — a `included do...end`-style idiom —
  which reframed the whole problem: the bottleneck isn't where a declaration
  comes from, it's that one idiom, blind to source.
- **Inspecting the body of that idiom's block was itself measured and
  rejected, and its own motivating premise was already stale.** A census over
  the real gem source found that none of its `included do` blocks were inert —
  every one was a generator (a class-level attribute/method-generation helper
  dominated the inconclusive cases) — so inspecting block bodies would close
  zero of them under any fail-closed rule. A real probe measured the call-site
  yield at roughly 0.005-0.06% across the corpora, essentially nothing, with
  zero diagnostics moved. **The bead's own motivating bucket number had also
  gone stale**: the idiom's bucket had been measured at over 10% of call sites
  on the reference corpus when the bead was proposed; by the time it was
  built, a *later* capability (the RBI-resolution lever) had already drained
  that same bucket more than 20x, unrelated to this bead. **"A bucket number
  in this document ages; every child ticket re-measures the number that
  motivated it before writing code" (learned, binding — already a rule
  above)** — here that discipline cost one query and saved building against a
  lever more than 20x smaller than assumed. A side finding reordered the next
  step entirely: on the reference corpus, the dominant class-body-block idiom
  wasn't the framework's own concern pattern at all, but a public GraphQL
  gem's field-declaration DSL (the large majority of all such openings),
  followed distantly by a client-internal event-watcher DSL (named here only
  as "a client-internal DSL", per the secrecy wall — its actual names exist
  only in one client's code and never enter this file). Without that census
  the fix target would have been wrong by roughly 20x. **"Ticket text that
  contradicts an already-proven allowlist: the allowlist wins, and the
  divergence is recorded" (learned, binding)** — this ticket's own acceptance
  criteria listed a method that the allowlist had already proven inert as one
  that should "still open"; the implementation followed the allowlist, proven
  correct by mutation, over the ticket's stale assumption.
- **Curated ActiveRecord-API declaration, shipped**: a generated inventory of
  the real gem's public instance and singleton method names, resolving to
  `Ty::Unknown` only (no synthesized method definition, so arity is never
  checked — the same safety guarantee as the RBI-resolution lever).
  **Near-miss caught before shipping**: the new lookup step had inherited an
  early return gated on "an RBI map exists", which would have zeroed it
  entirely on the two corpora that don't ship an RBI tree — exactly the two
  corpora it was built for; moved outside that gate before shipping. Measured,
  anchored on multiple machines:

  |corpus|new bucket|% of call sites|blind before -> after|errors/warnings|
  |---|---|---|---|---|
  |corpus-c|2791|1.9%|117164 -> 114373|3 / 1525|
  |corpus-a|**2899**|**4.5%**|53416 -> 50517|2 / 603|
  |corpus-b|**498**|**6.1%**|7085 -> 6587|0 / 92|

  **The prediction matched the call site exactly** — an earlier
  attribution-by-callee-name estimate had predicted 2899/498 before the code
  existed; the measured values were 2899/498. What's left is measured too: the
  remainder is 58-61% generated attribute/association methods or a client
  method that no static declaration can name — the honest ceiling of this
  whole declaration-based approach.
- **A filesystem-enumeration-order bug in the RBI loader**: when multiple RBI
  files declared the same class (the real gem's file plus one or more
  reopening stubs), a first-wins map meant the WINNER depended on unstable
  directory-enumeration order — if a small stub won, the ancestry-closure
  computation silently degraded from over a hundred edges to single digits,
  and because the degraded result is still safe under invariant #1, it was
  completely invisible. Fixed by unioning every declaring file instead of
  picking one (mirroring a pattern the codebase already used elsewhere, with
  the exact prior reasoning already written down: first-wins would drop a
  second file reopening the same core class). Measured on corpus-c:

  |metric|before this fix|after this fix|
  |---|---|---|
  |static-declaration bucket|2791|**0**|
  |RBI-live-walk bucket|—|14115 (9.7%)|
  |DSL-RBI bucket|—|16449 (11.3%)|
  |blind total|114373|**103528** (-10845)|
  |errors|3|3 (hash identical)|
  |warnings|1525 (ceiling)|**373**|

  The static bucket falling to zero doesn't mean it was removed — it means the
  live RBI walk now always reaches the full real file regardless of directory
  order, so those calls resolve one step earlier in the escalation chain. The
  warning ceiling drop is the identical mechanism applied to unresolved
  constants: a real constant whose declaration had been losing the filesystem
  race sat in warning by enumeration accident, not by a real error. **The
  ratchet tightening in the very same commit is itself proof the bug was
  ACTIVE on this machine, not latent** — if directory order had already
  favored the full file, the ceiling would not have moved.

### J. Cross-cutting lessons not already listed as binding rules above

- A grep-based instrument shipped without a silence proof produced 42 false
  accusations across 120 real issues in one unrelated repo (40 of 40 issues in
  that repo falsely flagged) — the origin measurement behind the two-sided
  proof rule already listed above.
- Measuring on the wrong machine returns absence, and absence reads as a
  negative fact: a finding based on a corpus not being present on the
  measuring machine survived by a different path, but its stated premise was
  false. Every corpus number carries the machine it was collected on, or it
  doesn't count.
- A generated, versioned tracker export can leak through its comment fields,
  not just title and description — the fifth secrecy-wall surface (a
  tool-generated, versioned artifact) covers every field a tool emits, and a
  field with no delete capability makes prevention the only real defense.
  Superseded now that the tracker is GitHub Issues rather than a locally
  exported database, but the general rule — audit every field of a generated
  artifact, not just the obvious ones — still applies to any future exporter.

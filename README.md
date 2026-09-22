# itaruby

*itá* means "stone" in Tupi — itaruby looks at the stone and says whether it
is a real ruby. It is a Ruby type checker written in Rust, inference-first
(in the style of [ty] and [Pyrefly], not Sorbet): no annotations required,
and fast enough to stream diagnostics as you type.

[ty]: https://github.com/astral-sh/ty
[Pyrefly]: https://github.com/facebook/pyrefly

Findings, benchmark and status: <https://aryrabelo.com/en/itaruby/>.

## What it is

ruby-lsp has no type system of its own — its diagnostics are syntax (via
prism) plus lint rules, and its `TypeInferrer` resolves literals/`self`/
constants and otherwise guesses from a capitalized variable name. Its own
roadmap names the gap it doesn't fill yet: "Allow the Ruby LSP to connect to
a typechecker add-on to improve accuracy," with no tracking issue and no
implementation. ruby-lsp already has a deference mechanism for this
(`GlobalState#detect_typechecker`); it just only recognizes `sorbet-static`
today.

itaruby is built to be that add-on: inference-first, zero annotations
required, real-time. It ships as:

- `ita check <path>...` — batch CLI, human-readable output plus
  machine-readable `--format=json` and `--format=agent` modes.
- `ita server` — a real-time LSP server (push and pull diagnostics,
  `textDocument/hover`, `textDocument/definition`).
- `ita definition <path>:<line>:<col>` / `ita hover <path>:<line>:<col>` —
  the scriptable, one-shot CLI twins of the LSP requests.
- `scripts/ruby-lsp-itaruby` — a ruby-lsp addon gem that plugs `ita server`
  into a running ruby-lsp session as a diagnostics source.
- `scripts/vscode` — a reference VS Code extension for editors that don't
  run ruby-lsp.

## Status

Alpha, pre-1.0. The core contract already holds — see "Invariant #1" below —
but the checker doesn't yet handle autoload, refinements, or block
parameter types, and the launch bar (below) hasn't been cleared yet.

The class-object track armed on 2026-09-21: a call on a project class
object (`Page.find_by_slog`) is now checked the same way a call on an
instance is, gated on the receiver's whole singleton ancestry being
closed. It shipped as a measurement — the dark-singleton census bucketed
every would-be accusation on the three public corpora until the residue
was 8 records across rails/mastodon/discourse, each read and proven to
raise under MRI.

## Quick start

```sh
git clone https://github.com/aryrabelo/itaruby.git
cd itaruby
cargo build --release
./target/release/ita check app/          # batch; exit 1 if there's an Error
./target/release/ita server              # LSP over stdio
```

## Invariant #1

`Unknown` never produces a diagnostic. Anything not inferable becomes
`Unknown` and stays silent: a false negative is acceptable, a false
positive is not. Classes touched by metaprogramming (`method_missing`,
dynamic `define_method`, DSLs, blocks in the class body, dynamic
superclass, `raise NotImplementedError`) are treated as `open` — no
diagnostic about them. Navigation follows the same rule: no answer is
fine, a wrong answer is not.

## Diagnostics (v0)

| code | severity | what |
|---|---|---|
| E0101 | Error | `undefined method` on an instance of a project class, or on a project CLASS OBJECT (closed ancestor chain on either track) |
| E0102 | Error | wrong arity (positional) |
| E0103 | Error | argument type incompatible with an inline RBS sig |
| E0104 | Warning | unresolved constant |
| E0105 | Warning | malformed `#:` comment (the sig is ignored; the method falls back to inference) |
| E0106 | Warning | string literal assigned to a column whose schema type won't take it |
| E0107 | Warning | a local's inferred usage contradicts every candidate type |
| E0108 | Error | operator operand pairing MRI raises `TypeError` on, both operands proven from literals in one scope |
| E0001 | Error | syntax error (prism, error-tolerant) |

itaruby reads whatever else exists for free: Tapioca RBIs (`sorbet/rbi/`,
lazily loaded — only the file for the demanded constant is parsed),
`db/schema.rb`/`db/structure.sql`, and inline RBS sigs (`#:`).

## Inline RBS sigs (optional)

```ruby
#: (Integer, ?String, foo: Symbol) -> Array[String]
def build(count, label = nil, foo:)
  # ...
end
```

## Benchmarked against Sorbet

itaruby is measured against Sorbet on four public Rails codebases —
[rails/rails], [mastodon], [discourse], and [gitlab-foss] — on two
dimensions: wall-clock speed and true bugs found. The launch bar for 1.0 is
winning on both, on every one of the four repos.

[rails/rails]: https://github.com/rails/rails
[mastodon]: https://github.com/mastodon/mastodon
[discourse]: https://github.com/discourse/discourse
[gitlab-foss]: https://gitlab.com/gitlab-org/gitlab-foss

As of the last measurement round: itaruby is faster on 2 of the 4 repos
(discourse, gitlab-foss) and slower on the other 2 (rails, mastodon) against
`srb tc --typed true`. A reciprocal false-negative audit (240 sampled
candidates, 60 per repo) found zero true positives that Sorbet caught and
itaruby missed. The difference isn't just the engine — it's the model:
Sorbet checks what's annotated, and under `typed: false`, where most
production Rails code lives, it never looks inside a method body. itaruby
infers first and only speaks when it's sure (Invariant #1), so it finds
bugs in code that has never seen a sig.

The annotation-free bench (`scripts/inference-bench.jsonl`, judged by
`scripts/inference-gate.sh`, described in `scripts/inference-bench-README.md`)
asks a different question from the corpus rounds: on twelve self-contained,
gem-free fixtures, does itaruby find real bugs with **zero annotations**,
and does it stay silent on dynamic code that is correct at runtime? MRI is
the judge — every accusation must really raise on the blamed line, every
silence must really exit 0 — and the ledger publishes both directions.
Until 2026-09-21 **Sorbet won two rows**, `extend_singleton_typo` and
`included_hook_class_method_typo`: singleton and hook shapes where the
annotated leg proves a certain `NoMethodError` and itaruby stayed silent.
A blanket fix had been reverted for a measured reason (216/0/12 false
positives on rails/mastodon/discourse); the populations that had to be
indexed first (`mattr_accessor`/`cattr_accessor`, `class << self`
`attr_*`, stdlib module functions, and the rest of the twelve named
mechanisms) were indexed, and the class-object flip closed both rows.
That column now reads zero. Conversely, on the three rows where Sorbet
needs an annotation, the only claim is the narrow one — itaruby does not
false-positive there; each such silence carries a positive control with a
certain typo planted, so silence is never claimed as understanding. The
bench also caught a live Invariant #1 violation once —
`const_missing_namespace`, an E0104 on a namespace defining
`self.const_missing` — fixed the day it was found.

## Ruby LSP addon

`scripts/ruby-lsp-itaruby` plugs itaruby into a running
[ruby-lsp](https://github.com/Shopify/ruby-lsp) session as a diagnostics
source, discovered through ruby-lsp's standard add-on mechanism. On
activation it resolves the `ita` binary and registers as a ruby-lsp
formatter; on first use it spawns one persistent `ita server` process for
the whole session and pulls `textDocument/diagnostic` instead of shelling
out to `ita check` per keystroke. Any failure — missing binary, dead child
process, malformed response — degrades to "no diagnostics" plus a log
message, matching itaruby's own invariant that silence is always safe.
Details and setup: `scripts/ruby-lsp-itaruby/README.md`.

## VS Code extension

`scripts/vscode` is a thin LSP client: starts `ita server` over stdio and
wires it to any open `.rb` file. No bundled binary, no marketplace listing —
build and install locally. Details: `scripts/vscode/README.md`.

## Architecture

```
crates/
├── itaruby_syntax/     # SourceFile (salsa input) + LineIndex; parsing via ruby-prism
├── itaruby_semantic/   # definition index, types, inference, diagnostics
│   ├── types.rs      # Ty (Unknown/Int/Str/.../Instance/Union) — invariant #1
│   ├── index.rs      # file_defs (per file) → project_index (global merge)
│   ├── core.rs       # static table of core methods (Integer/String/Array/...)
│   ├── rbs_comment.rs# parser for `#:` comments
│   └── check.rs      # flow-sensitive walk + diagnostics
├── itaruby_server/     # LSP (lsp-server, synchronous loop, cancellation via salsa)
└── itaruby/            # binary: check + server
```

Incrementality: `file_defs` depends only on its own file's text; editing a
method body without changing its signature produces a structurally
identical `FileDefs`, and salsa's early-cutoff doesn't invalidate the
global index. The LSP's debounce is salsa's cancellation: a new
`didChange` cancels the in-flight query.
For repeated use, run `ita server`: the long-lived LSP process is the
cache — salsa recomputes only what an edit actually invalidates.
`ita check` is deliberately stateless: cold start every run, nothing
written to disk. A persistent result cache was measured and rejected
(2026-08-26): per-file invalidation is unsound in Ruby (any file can
reopen any class, so one edit can change every diagnostic), and a
whole-project snapshot pays ~26% of a run just to learn "nothing
changed" — for a near-zero hit rate in every real workload (CI runs
once per push; the binary version is part of any honest key).

v0 doesn't handle: autoload, refinements, ivar narrowing (narrowing is
local- and parameter-only by design — any method can mutate an ivar), and
block parameter types. `require` is consulted only to decide whether a
stdlib constant is in scope, never to scope the index, which stays
workspace-global. A typo in a core method is conclusive only when the set
of installed gems is provably known; otherwise it stays silent. Everything
here degrades to `Unknown` — never to an error.

## Development

```sh
./scripts/dev setup && ./scripts/dev gates
```

`./scripts/dev setup` prepares the machine (git hooks, release build,
[software-factory], the local issue tracker) and prints which gates run
here. `./scripts/dev gates` delegates to `scripts/gauntlet-gates.sh`: it
runs [software-factory]'s checks, `cargo test --workspace`, the release
build, the mutation probes under `testdata/`, the unwrap-shape gate, and
the performance ceiling — all of which need only this repository and run on
any machine.

[software-factory]: https://github.com/nicolasmelo1/software-factory

A separate gate diffs the checker's output against a private validation
corpus that never leaves the machine that hosts it; that gate needs no
special setup to skip cleanly — `./scripts/dev gates` exits `2` (not `1`)
when it's unavailable, and every other gate still runs and is checked. See
`AGENTS.md` for the exact rules and `.github/CONTRIBUTING.md` for the
contribution workflow.

### The bar, if you're reading the code

The toolchain is pinned in `rust-toolchain.toml` so "compiles here" and
"compiles in CI" are the same claim. The lint policy lives in
`[workspace.lints]` rather than in CI flags, for the same reason:
`clippy::pedantic` is denied and `unsafe_code` is forbidden — the repository
contains no `unsafe` at all. Five lint families are allowed at the
workspace level and each one states its reason in `Cargo.toml`; the rest of
the exceptions are `#[expect]` at call sites, chosen over `#[allow]` so a
suppression that stops being needed fails the build instead of rotting.

Two things a reader usually has to take on faith are checked here instead.
Every `unwrap()` in production source is a ruby-prism downcast inside a
match arm that already proved it — `scripts/unwrap-gate.sh` fails on any
other shape, so "none of these can panic" is a property rather than a
claim. And `scripts/perf-gate.sh` compares criterion medians against
committed ceilings in `scripts/perf-baseline.txt`, because a project whose
launch bar is speed should notice a regression before a reviewer does.

## License

MIT — see `LICENSE`.

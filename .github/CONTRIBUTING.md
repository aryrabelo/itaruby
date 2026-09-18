# Contributing to itaruby

itaruby is a Rust-based, inference-first type checker for Ruby, shipped as a
typechecker add-on for [ruby-lsp](https://github.com/Shopify/ruby-lsp)
rather than a standalone editor plugin. See `AGENTS.md` for the full design
rationale.

## Getting set up

```sh
git clone git@github.com:aryrabelo/itaruby.git
cd itaruby
./scripts/dev setup
```

`./scripts/dev setup` is idempotent: it wires the git hooks
(`core.hooksPath` -> `.githooks`), builds the release binary, installs
[`software-factory`](https://github.com/nicolasmelo1/software-factory)
(`sf`), and initializes the local `bd` (beads) issue tracker. It also
reports which verification gates this machine can run.

## Verifying a change

```sh
./scripts/dev gates
```

This runs `sf check`, `cargo test --workspace`, and the mutation probes
under `testdata/`. Every one of those requires only this repository — no
external data.

A second class of gate diffs a checked-out corpus of real-world Ruby code
against `scripts/corpus-baseline.txt` (warning ceilings, error-hash
identity). Those corpora are three private production Rails codebases that
never leave the machines that hold them, and no single machine holds all
three — so a complete proof of a change requires running the gates on two
machines (see `AGENTS.md`, "Multi-machine" section, for the exact split).
`./scripts/dev gates` exits `2`, naming which declared corpus is absent,
when run on a machine that can't see one. That is expected on a
contributor's laptop, not a failure to fix locally.

## Invariant #1 (inviolable)

`Ty::Unknown` never generates a diagnostic. A false negative (missing a
real bug) is acceptable; a single false positive against any of the
corpora above is a shipped defect. This applies to every diagnostic code
and to navigation (`definition`/`hover`): no answer is fine, a wrong
answer is not.

## Adding a new check, lint, or gate

Any new instrument (diagnostic rule, lint, grep pattern, CI gate) ships
only after proving both sides:

- it fires under a real mutation of correct code (the defect it targets
  actually triggers it), and
- it stays silent on correct code (nothing correct trips it by accident).

The probes that prove each side ship alongside the instrument (see
`testdata/` for fixtures, or inline `#[cfg(test)]` modules for pure
functions) — not just described in a PR description. Without the probes
next to the code, the boundary looks like decoration and the next session
removes it.

## Policy

[`software-factory`](https://github.com/nicolasmelo1/software-factory)
(`sf check`) runs in the pre-commit hook and in CI
(`.github/workflows/software-factory.yml`). Loosening
`.software-factory/policy.yaml` or its ratchet to make a check pass is not
allowed — a rule that blocks your change is a finding about the change,
not about the rule.

## Confidentiality

No file in this repository may contain a path, symbol, class, method,
table, or column name from any client's private codebase. Public gem API
names (e.g. `field`, `argument`, `validates`) are fine; anything that only
exists in a specific client's code is not.

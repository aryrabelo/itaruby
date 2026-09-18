# The ruby-lsp add-on: install and best practices

Validated on 2026-09-03: ruby-lsp 0.26.11, Ruby 3.4.2 (`arm64-darwin25`),
itaruby commit `0e64c5e`, Apple M5 / macOS 25.6. End-to-end means a real
`ruby-lsp` process over stdio, driven by a hand-rolled client: `initialize`
with `initializationOptions`, `didOpen`, `textDocument/diagnostic`, and the
E0101 diagnostic observed coming back through ruby-lsp with the exact range.
Setup not reproduced here (VS Code GUI, Neovim, Zed) is marked as such.

## What it is

ruby-lsp has no type system — its own diagnostics are syntax plus lint rules,
and its docs point users who want types at Sorbet or Steep. itaruby plugs in
as a diagnostics source through ruby-lsp's standard add-on mechanism: a small
gem (`scripts/ruby-lsp-itaruby`) that ruby-lsp discovers on boot and that
bridges one persistent `ita server` process into the session. Silence is
always safe — itaruby's [Invariant #1](../README.md#invariant-1) (`Unknown`
never produces a diagnostic) holds through the bridge: every failure mode on
the way to your editor degrades to no diagnostics plus a log line, never to a
wrong one.

## Install

### 1. Build the checker

```sh
git clone https://github.com/aryrabelo/itaruby.git
cd itaruby
cargo build --release          # produces target/release/ita
```

Make `ita` reachable in one of three ways (all verified end-to-end):

1. On `PATH` (the default lookup), or
2. `ITA_BIN=/abs/path/to/ita` in the environment ruby-lsp inherits from, or
3. per-workspace, via `initializationOptions.addonSettings.itaruby.bin` —
   see the editor configs below.

The addon checks `ITA_BIN` first, then `addonSettings`, then `PATH`. The
binary must support `ita server` with `textDocument/diagnostic` (LSP 3.17) —
any release of this repository does.

### 2. Add the add-on to the project's bundle

Not published on rubygems.org. Point Bundler at a checkout:

```ruby
# Gemfile
gem "ruby-lsp-itaruby", path: "/absolute/path/to/itaruby/scripts/ruby-lsp-itaruby"
```

```sh
bundle install
```

ruby-lsp requires add-ons to be part of the project's bundle (globally
installed linters/formatters are not supported) — this layout satisfies that.

### 3. Tell ruby-lsp to use it

ruby-lsp resolves diagnostics sources from `initializationOptions.linters`,
an array of registered formatter identifiers. The add-on registers as
`"itaruby"`, so:

**VS Code** — `.vscode/settings.json` (verified as a documented setting that
feeds `initializationOptions.linters`; the GUI path itself was not exercised
in this validation — the stdio protocol path below was):

```json
{ "rubyLsp.linters": ["itaruby"] }
```

**Neovim** (nvim-lspconfig, unverified in this session — `init_options` is
nvim-lspconfig's generic pass-through into `initialize`):

```lua
require("lspconfig").ruby_lsp.setup({
  init_options = { linters = { "itaruby" } },
})
```

For the binary path from Neovim, set `ITA_BIN` in the environment Neovim
spawns ruby-lsp from (e.g. `vim.env.ITA_BIN = "/abs/path/to/ita"` before
setup), or pass `addonSettings` as in the Zed example below.

**Zed and other clients** (unverified in this session): any client that lets
you set `initialization_options` for the ruby-lsp server works:

```json
{
  "lsp": {
    "ruby-lsp": {
      "initialization_options": {
        "linters": ["itaruby"],
        "addonSettings": { "itaruby": { "bin": "/abs/path/to/ita" } }
      }
    }
  }
}
```

`ITA_BIN` wins over `addonSettings` when both are set. Note the outer keys of
`addonSettings` are matched as strings and the inner keys as symbols — the
shape above (JSON from an editor) is what ruby-lsp parses and what the add-on
reads; it was verified working over stdio.

### What to expect on first open

- ruby-lsp boots, discovers the add-on via `Gem.find_files("ruby_lsp/**/addon.rb")`,
  and activates it; a first `ita server` spawn happens lazily on the first
  diagnostic request for an open file, not at boot.
- The Output panel → **Ruby LSP** channel shows, in order (exact lines from
  the validation run):
  - `Using linters specified by user: itaruby` — your config took effect.
  - `itaruby server: itaruby: no declaration sources found` (or one line per
    discovered declaration source) — what `ita server` picked up for the
    workspace.
- Diagnostics appear when the editor pulls `textDocument/diagnostic`. VS Code
  does this automatically for servers advertising the capability.

## How it works

- **Discovery and registration.** ruby-lsp loads the add-on on `initialized`
  and calls `activate`, which resolves the `ita` binary and registers a
  diagnostics formatter named `"itaruby"` in ruby-lsp's formatter registry.
  ruby-lsp then includes it in every diagnostics run for `.rb` documents
  (`GlobalState#active_linters`).
- **One persistent server per workspace.** The bridge spawns exactly one
  `ita server` child per ruby-lsp session, on the first diagnostic request,
  and speaks LSP 3.17 over its stdio: `initialize`/`initialized` handshake,
  `didOpen` on first sight of a file, full-sync `didChange` on every edit
  after, and a `textDocument/diagnostic` pull per request. The child lives
  for the whole session; `ita`'s salsa database is the incremental cache —
  an edit recomputes only what the edit invalidated.
- **Pull diagnostics.** The add-on answers ruby-lsp's formatter
  `run_diagnostic` contract with a synchronous pull round-trip to `ita
  server` and maps the returned items straight into LSP diagnostics (ranges
  arrive already 0-based from the server). Note the direction: ruby-lsp
  itself pushes nothing on edit (it only pushes an empty
  `publishDiagnostics` on file close); if your editor never sends
  `textDocument/diagnostic`, you see nothing — VS Code pulls by default.
- **Failure behavior.** If the child dies, the pipe times out (10 s read
  deadline per response), or a response is malformed, the bridge stops the
  process, respawns once (re-handshakes, re-opens the current document), and
  on a second consecutive failure disables itself for the rest of the
  session — returning empty diagnostics and logging why. A missing binary is
  additionally reported once at activation. Verified end-to-end with a
  bogus `ITA_BIN`:

  ```
  itaruby: binary '/nonexistent/dir/ita' not found (checked ITA_BIN, addonSettings.bin and PATH). Diagnostics will stay silent until it is installed.
  itaruby: server process failed (No such file or directory - /nonexistent/dir/ita), respawning
  itaruby: server crashed twice in a row, disabling itaruby diagnostics for this session (No such file or directory - /nonexistent/dir/ita)
  ```

  The host ruby-lsp process never sees an exception.

### Measured numbers (Apple M5, Ruby 3.4.2, ruby-lsp 0.26.11, itaruby `0e64c5e`)

| scenario | first diagnostic (cold `ita server`) | subsequent `didChange`→diagnostic |
|---|---|---|
| 1-file project | 162–230 ms (3 runs) | 1.1–6.6 ms |
| rails checkout (3,458 `.rb` files, read-only clone) | 746 ms | 2.4 ms |

Cold includes: spawning `ita server`, the LSP handshake, loading and indexing
the whole workspace tree, `didOpen`, and the first pull. For scale: a plain
`ita check` over the same 3,458-file tree takes 5.96 s wall on this machine —
the server's one-time workspace load is a fraction of a batch run, and every
diagnostic after the first is a round-trip, not a re-run. All numbers are far
inside the bridge's 10 s read deadline; timeouts are not a practical concern
at this scale.

## Best practices

### Adopt it as an advisory CI job first

Run the batch checker non-blocking before you let it gate anything:

```sh
ita check app/ --format=json    # JSONL, one object per diagnostic
# or, for an AI agent to resolve contradictions:
ita check app/ --format=agent
```

Exit codes: `0` clean, `1` at least one Error-severity diagnostic, `2` usage
problem or a checked path that doesn't exist (the run checked nothing — not
the same as clean). Wire it as a non-blocking job, read every diagnostic for
a week, fix the true positives and file the false ones, and only then make
the job blocking. The checker is inference-first and stays silent where it
cannot prove — but the correct response to a diagnostic you don't understand
is to read the flagged code, not to silence the tool.

### Keep declaration sources discoverable

`ita server` discovers, per workspace, by searching **upward** from the workspace
root (at most 4 directory levels):

- `db/schema.rb` (preferred) or `db/structure.sql` — powers E0106 and ActiveRecord
  attribute types; `schema.rb` wins when both exist, they are never merged;
- `sorbet/rbi/` — Tapioca-generated RBIs, parsed lazily per demanded constant;
- `Gemfile.lock` — gem names are mapped to guessed Ruby namespaces so gem
  classes your project reopens don't turn into false E0101s.

Consequences:

- Keep these files at (or above) the workspace root you open in the editor.
- Open the real project directory, not a symlink pointing into it: a root
  reached through a symlink walks up through the *link's* parent directories,
  where your `Gemfile`/`sorbet/rbi` may not exist — this repository records a
  measured incident where a symlinked root turned 2 real errors into 310
  fabricated ones. Failing open (silence) is the designed behavior; a
  symlinked workspace root triggers it.

### Use inline RBS sigs where you want deeper checks

```ruby
#: (Integer, ?String, foo: Symbol) -> Array[String]
def build(count, label = nil, foo:)
```

A `#:` sig gives the checker a contract: arguments are checked against it
(E0103) and a malformed sig is itself reported (E0105, and the method falls
back to inference). Without sigs, arity is still checked (E0102) but argument
*types* are only checked against explicit contracts.

### What not to expect

From the checker itself (each degrades to silence, by design):

- **Autoload** — Zeitwerk-style constant resolution is not modeled; constants
  outside the indexed workspace resolve only through discovered declarations.
- **Refinements** — not modeled.
- **Block parameter types** — blocks don't propagate types to their parameters.
- **Ivar narrowing** — instance variables are read as whole-class state;
  narrowing is local- and parameter-only (any method can mutate an ivar).

And one boundary of the bridge: diagnostics are **pull-only**. An editor that
doesn't send `textDocument/diagnostic` never sees itaruby output, no matter
what the logs say.

### Reading a diagnostic

| code | severity | means | what to do |
|---|---|---|---|
| E0101 | Error | `undefined method` on an instance of a project class with a closed ancestor chain | Fix the call; `ita check`'s excerpt names the did-you-mean candidate (the LSP message carries the plain text only) |
| E0102 | Error | wrong positional arity on a resolved call | Match the method's parameters |
| E0103 | Error | argument type contradicts an inline RBS `#:` sig | Fix the argument or the sig |
| E0104 | Warning | unresolved constant | Usually a real misspelling or a gem constant the workspace can't see; verify against `Gemfile.lock`/`sorbet/rbi` before dismissing |
| E0105 | Warning | malformed `#:` comment | Fix the sig syntax; until then the sig is ignored and the method is inferred |
| E0106 | Warning | string literal assigned to a column whose schema type won't take it | Check the column in `db/schema.rb`/`db/structure.sql` |
| E0107 | Warning | a local's inferred usage contradicts every candidate type | `ita check --format=agent` prints a self-contained contradiction block; pin the intended type with `#:` if the code is right |
| E0001 | Error | syntax error (error-tolerant prism parse) | Fix the syntax; other diagnostics resume after |

In the editor all of these come from source `itaruby`; severities map to
LSP Error=1, Warning=2.

### Multi-root workspaces

Each workspace folder gets its own `ita server` process and its own index;
a file outside the server's root is ignored whole (with a one-time
`window/logMessage` warning naming the path), and never merges into another
folder's project index. Open the folder that owns the code you want checked.

### Troubleshooting

- **No diagnostics at all.** Check the Ruby LSP output channel for
  `Using linters specified by user: itaruby`. If it's absent, your
  `linters` initialization option didn't reach the server — check the
  editor config, and that the gem is in the project's bundle.
- **`itaruby: binary '...' not found`** at activation. `ITA_BIN` is unset or
  wrong, `addonSettings.itaruby.bin` is unset, and `ita` is not on the
  `PATH` ruby-lsp inherited. Fix any one of the three. Remember VS Code
  derives ruby-lsp's environment through your Ruby version manager — a
  binary exported in a different shell's profile may not be visible.
- **Diagnostics stop after an error, permanently.** The bridge disables
  itself after two consecutive child failures (`server crashed twice in a
  row, disabling itaruby diagnostics for this session`). Restart the ruby-lsp
  server (VS Code: Command Palette → "Ruby LSP: Restart") after fixing the
  underlying cause (bad `ita` build, binary without `ita server` support).
- **An older `ita` binary without pull diagnostics.** It spawns, fails the
  handshake or the first pull, respawns once, and disables for the session —
  same visible shape as the missing-binary case above (log lines verified for
  the missing-binary path; the old-binary path follows the same code and was
  not separately exercised).
- **Where logs live.** The Output panel → **Ruby LSP** channel. Everything
  the add-on and `ita server` report arrives as `window/logMessage`
  notifications; ruby-lsp prefixes `ita server`'s own lines with
  `itaruby server:`.
- **Diagnostics visible in `ita check` but not the editor.** The editor only
  sees the open file (`ita check` checks whole trees), and the checker stays
  silent on classes it can't prove closed. If `ita check path/to/file.rb`
  reports and the editor doesn't, confirm the file is inside the opened
  workspace folder.

## Known limitations of the add-on itself

- ruby-lsp's formatter registry is a single namespace shared by formatters
  and linters, and the add-on rides the formatter contract
  (`run_diagnostic`) that Shopify ships today. Whether `linters` accepting a
  third-party formatter identifier is intended, supported usage — versus
  something needing an upstream conversation — remains open.
- The `ita` binary is resolved **once, at activation**. Changing `ITA_BIN` or
  `addonSettings` requires a server restart.
- Built and validated against ruby-lsp ≥ 0.23, < 0.27 (guarded by
  `RubyLsp::Addon.depend_on_ruby_lsp!` — a mismatch logs a warning and skips
  activation rather than breaking boot). End-to-end validated on 0.26.11
  only.
- Validated with a hand-rolled stdio client on macOS/ARM. One observation
  from that client, for anyone writing their own: ruby-lsp 0.26.11 applies
  `didChange` edits incrementally and errors on a range-less full-document
  `contentChange` despite advertising FULL sync — real editors always send
  ranged edits, and the add-on is unaffected either way (its own sync with
  `ita server` is genuinely full-text).

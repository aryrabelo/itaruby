# ruby-lsp-itaruby

Prototype addon that plugs [itaruby](../../README.md) (`ita`, this repo's Ruby
type checker) into [ruby-lsp](https://github.com/Shopify/ruby-lsp) as a
diagnostics source. This is the "bridge" prototype from the itaruby roadmap
epic — proof that itaruby can ride inside an editor's existing ruby-lsp
session instead of shipping its own LSP client, before deciding whether that
becomes the real integration path.

## What it does

- Discovered by ruby-lsp via its standard add-on mechanism
  (`Gem.find_files("ruby_lsp/**/addon.rb")` — see `lib/ruby_lsp/itaruby/addon.rb`).
- On activation, resolves the `ita` binary (`ITA_BIN` env var, then
  `initializationOptions.addonSettings.itaruby.bin`, falling back to `ita` on
  `PATH`) and registers it as a ruby-lsp formatter named `"itaruby"`.
- On the first `run_diagnostic` call, `ServerClient` lazily spawns **one**
  persistent `<bin> server` child process for the whole ruby-lsp session and
  speaks standard LSP 3.17 over its stdio: `initialize`/`initialized`, then
  `textDocument/didOpen` (first sight of a file) or `textDocument/didChange`
  (full-sync, every call after) followed by a `textDocument/diagnostic` pull
  request. Diagnostic items come back with real, server-computed ranges and
  are mapped straight into `RubyLsp::Interface::Diagnostic` — no more
  shelling out to `ita check` per keystroke.
- If the child process dies or its pipe times out, `ServerClient` respawns it
  once (re-handshaking and re-opening the current document). A second
  consecutive failure disables the client for the rest of the session:
  `run_diagnostic` then returns `[]` immediately, plus one `window/logMessage`
  explaining why.
- `run_formatting` / `run_range_formatting` always return `nil` — itaruby
  never formats code, only reports diagnostics.
- Never raises into the host: a missing/broken binary, a dead child, or a
  malformed response all degrade to "no diagnostics" plus a
  `window/logMessage` notification, matching itaruby's invariant that silence
  is always safe and a wrong answer never is.

## Install (local prototype)

Not published. Point Bundler at the local path:

```ruby
gem "ruby-lsp-itaruby", path: "/absolute/path/to/itaruby/scripts/ruby-lsp-itaruby"
```

Then in your editor's ruby-lsp config (e.g. VS Code `settings.json`):

```json
{ "rubyLsp.linters": ["itaruby"] }
```

Set `ITA_BIN` if `ita` isn't on `PATH`.

**Requires an `ita` build with `ita server` support for
`textDocument/diagnostic` pull requests** (LSP 3.17). Older `ita` binaries
that only support `ita check` will spawn, fail the handshake or the first
pull request, get one respawn attempt, and then disable diagnostics for the
session — silently, per itaruby's invariant, with a `window/logMessage`
explaining why.

## Known limitation

ruby-lsp resolves the active **formatter** (not linter) per document from a
single configured entry, and the addon/formatter registration API this
prototype relies on is the one Shopify ships today (`RubyLsp::Addon`,
`RubyLsp::Requests::Support::Formatter#run_formatting` /
`#run_range_formatting` / `#run_diagnostic`, `GlobalState#register_formatter`).
Whether `rubyLsp.linters` accepting a third-party formatter identifier like
`"itaruby"` cleanly is the intended, supported usage — versus something that
needs an upstream conversation with Shopify — is still open. This addon is
built against that surface as observed in ruby-lsp 0.23.0, and was validated
end-to-end on 2026-09-03 against ruby-lsp 0.26.11 (Ruby 3.4.2, itaruby
commit `0e64c5e`, Apple M5): a real `ruby-lsp` process over stdio discovered
the addon, activated it, and returned an itaruby E0101 with the correct
range through `textDocument/diagnostic` — cold first diagnostic ~0.16–0.23 s
on a one-file project and ~0.75 s on the rails checkout, subsequent
didChange round-trips 1–7 ms. Details, measured numbers, install paths and
best practices: [docs/ruby-lsp-addon.md](../../docs/ruby-lsp-addon.md).

## Testing

No dependency on the real `ruby-lsp` gem being installed — `test/test_helper.rb`
defines minimal stand-ins for the namespaces this addon touches, shaped after
the real gem's sources, plus a fake `ita server` (also in `test_helper.rb`)
that speaks real LSP framing so `ServerClient`/`DiagnosticRunner` are
exercised end to end without the real `ita` binary.

```console
$ ruby -Itest -Ilib test/addon_test.rb
$ ruby -Itest -Ilib test/server_client_test.rb
```

### Testing against a real ruby-lsp in VS Code

1. Build `ita` (`cargo build --release -p itaruby` from the repo root) and
   make sure it supports `ita server` + `textDocument/diagnostic`.
2. In the target Ruby project's `Gemfile`, add:
   ```ruby
   gem "ruby-lsp-itaruby", path: "/absolute/path/to/itaruby/scripts/ruby-lsp-itaruby"
   ```
   then `bundle install`.
3. In that project's VS Code `settings.json`:
   ```json
   { "rubyLsp.linters": ["itaruby"] }
   ```
   ruby-lsp does not expose a setting to inject arbitrary env vars into its
   process — it derives its environment by activating your shell's Ruby
   version manager (`rubyLsp.rubyVersionManager` in `settings.json`, `"auto"`
   by default). So set `ITA_BIN` (if `ita` isn't already on `PATH`) in the
   shell profile that manager sources (e.g. `~/.zshrc` for a login shell), or
   symlink the built `ita` binary onto a directory already on `PATH`.
4. Reload the window (or restart the Ruby LSP server: Command Palette →
   "Ruby LSP: Restart"). Open a `.rb` file — diagnostics should appear as you
   edit.
5. Logs (including itaruby's own `window/logMessage` notifications and the
   `ita server` handshake/respawn/disable messages) land in the **Output**
   panel → **Ruby LSP** channel, not the regular Debug Console.


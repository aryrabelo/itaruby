# itaruby VS Code extension

Thin LSP client: starts `ita server` over stdio and wires it to any open
`.rb` file. No bundled binary, no marketplace listing — build and install
locally.

## Build

```sh
cd scripts/vscode
npm install
npm run compile
```

## Package

```sh
npx vsce package
```

Produces `itaruby-vscode-<version>.vsix` in this directory.

## Install

```sh
code --install-extension itaruby-vscode-<version>.vsix
```

## Configure

`itaruby.serverPath` (VS Code setting) — defaults to `ita`, resolved from
`PATH`. Set it to an absolute path if `ita` is not on `PATH` (e.g.
`/Users/you/Sites/itaruby/target/release/ita`). If the binary cannot be
found, the extension shows an error message naming the path it looked for
instead of failing silently.

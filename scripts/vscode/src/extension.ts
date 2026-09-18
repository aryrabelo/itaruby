import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { LanguageClient, TransportKind } from "vscode-languageclient/node";
import type { LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(_context: vscode.ExtensionContext): void {
  const serverPath = vscode.workspace.getConfiguration("itaruby").get<string>("serverPath", "ita");
  const isBareCommand = !serverPath.includes(path.sep) && !path.isAbsolute(serverPath);
  const pathDirs = (process.env.PATH ?? "").split(path.delimiter);
  const found = isBareCommand
    ? pathDirs.some((dir) => dir.length > 0 && fs.existsSync(path.join(dir, serverPath)))
    : fs.existsSync(serverPath);

  if (!found) {
    vscode.window.showErrorMessage(
      `itaruby: could not find the "ita" binary (looked for "${serverPath}"). ` +
        `Install itaruby and ensure it is on your PATH, or set "itaruby.serverPath" ` +
        `to its absolute path in your VS Code settings.`,
    );
    return;
  }

  const serverOptions: ServerOptions = { command: serverPath, args: ["server"], transport: TransportKind.stdio };
  const clientOptions: LanguageClientOptions = { documentSelector: [{ scheme: "file", language: "ruby" }] };
  client = new LanguageClient("itaruby", "itaruby", serverOptions, clientOptions);
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}

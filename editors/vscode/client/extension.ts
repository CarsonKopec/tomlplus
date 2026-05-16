import * as fs   from "fs";
import * as os   from "os";
import * as path from "path";
import { workspace, ExtensionContext, window, OutputChannel } from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
    const channel: OutputChannel = window.createOutputChannel("TOML+");
    context.subscriptions.push(channel);

    const config       = workspace.getConfiguration("tomlplus");
    const configured   = config.get<string>("serverPath", "").trim();
    const resolvedPath = resolveServerPath(configured, context.extensionPath, channel);

    if (!resolvedPath) {
        const msg = "tomlplus-lsp binary not found. Run "
                  + "`cargo install --path crates/tomlplus-lsp` "
                  + "or set the `tomlplus.serverPath` setting to the full path of the binary.";
        window.showErrorMessage(msg);
        channel.appendLine(msg);
        return;
    }

    channel.appendLine(`Using language server: ${resolvedPath}`);

    const serverOptions: ServerOptions = {
        run:   { command: resolvedPath, transport: TransportKind.stdio },
        debug: { command: resolvedPath, transport: TransportKind.stdio },
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "tomlplus" }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher("**/*.tomlp"),
        },
        outputChannel: channel,
    };

    client = new LanguageClient(
        "tomlplus",
        "TOML+ Language Server",
        serverOptions,
        clientOptions,
    );

    try {
        await client.start();
        channel.appendLine("Language server started.");
    } catch (err) {
        const msg = `tomlplus-lsp failed to start: ${err}`;
        window.showErrorMessage(msg);
        channel.appendLine(msg);
    }
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}

/**
 * Resolve the language-server binary. Order is chosen so a fresh
 * `cargo install` of *our* binary always wins over unrelated executables
 * that happen to share the name (e.g. a Python `tomlplus-lsp.EXE` in a
 * pip Scripts directory that sits earlier on PATH).
 *
 *   1. The user's `tomlplus.serverPath` setting (explicit override).
 *   2. `~/.cargo/bin/tomlplus-lsp[.exe]`     ← where `cargo install` lands.
 *   3. Workspace `target/release/tomlplus-lsp[.exe]`.
 *   4. Workspace `target/debug/tomlplus-lsp[.exe]`.
 *   5. `tomlplus-lsp[.exe]` on PATH (last resort).
 *
 * Returns an absolute path if found, else `undefined`.
 */
function resolveServerPath(
    configured: string,
    extensionPath: string,
    channel: OutputChannel,
): string | undefined {
    const exeName = process.platform === "win32" ? "tomlplus-lsp.exe" : "tomlplus-lsp";

    // 1. Configured path (explicit override)
    if (configured) {
        const expanded = expandHome(configured);
        if (path.isAbsolute(expanded) && fs.existsSync(expanded)) {
            return expanded;
        }
        const onPath = findOnPath(configured);
        if (onPath) return onPath;
        channel.appendLine(`Configured tomlplus.serverPath="${configured}" not found.`);
    }

    // 2. ~/.cargo/bin — our canonical install location
    const cargoBin = path.join(os.homedir(), ".cargo", "bin", exeName);
    if (fs.existsSync(cargoBin)) return cargoBin;

    // 3 & 4. Workspace builds (extensionPath = editors/vscode → ../../ is the repo root)
    const workspaceRoot = path.resolve(extensionPath, "..", "..");
    for (const profile of ["release", "debug"]) {
        const p = path.join(workspaceRoot, "target", profile, exeName);
        if (fs.existsSync(p)) return p;
    }

    // 5. PATH (last resort — may hit unrelated binaries with the same name)
    const onPath = findOnPath(exeName);
    if (onPath) return onPath;

    return undefined;
}

function expandHome(p: string): string {
    if (p.startsWith("~")) {
        return path.join(os.homedir(), p.slice(1));
    }
    return p;
}

function findOnPath(name: string): string | undefined {
    const pathEnv = process.env.PATH ?? "";
    const sep     = process.platform === "win32" ? ";" : ":";
    const exts    = process.platform === "win32"
        ? (process.env.PATHEXT ?? ".EXE;.CMD;.BAT").split(";")
        : [""];

    for (const dir of pathEnv.split(sep)) {
        if (!dir) continue;
        for (const ext of exts) {
            const full = path.join(dir, name + (name.toLowerCase().endsWith(ext.toLowerCase()) ? "" : ext));
            try {
                if (fs.existsSync(full) && fs.statSync(full).isFile()) {
                    return full;
                }
            } catch { /* ignore */ }
        }
    }
    return undefined;
}

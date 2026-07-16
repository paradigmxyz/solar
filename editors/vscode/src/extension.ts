import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  State,
  TransportKind,
} from "vscode-languageclient/node";
import { spawn } from "child_process";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration("solarLsp");

  if (!config.get("enable")) {
    return;
  }

  // Start the LSP server
  startLanguageServer(context);

  // Register format document command
  const formatCommand = vscode.commands.registerCommand(
    "solarLsp.formatDocument",
    async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || editor.document.languageId !== "solidity") {
        return;
      }

      if (serverSupportsDocumentFormatting()) {
        await vscode.commands.executeCommand("editor.action.formatDocument");
        return;
      }

      const edit = await formatDocumentWithForge(editor.document);
      if (edit) {
        await editor.edit((builder) => {
          builder.replace(edit.range, edit.newText);
        });
      }
    },
  );

  // Preserve the legacy setting, deferring to VS Code when the server supports formatting.
  const formatOnSave = vscode.workspace.onWillSaveTextDocument((event) => {
    const currentConfig = vscode.workspace.getConfiguration("solarLsp");
    const editorFormatOnSave = vscode.workspace
      .getConfiguration("editor", event.document.uri)
      .get<boolean>("formatOnSave", false);
    if (
      currentConfig.get("formatOnSave") &&
      (!editorFormatOnSave || !serverSupportsDocumentFormatting()) &&
      event.document.languageId === "solidity"
    ) {
      event.waitUntil(formatDocument(event.document));
    }
  });

  const configListener = vscode.workspace.onDidChangeConfiguration((event) => {
    if (event.affectsConfiguration("solarLsp.enable")) {
      const currentConfig = vscode.workspace.getConfiguration("solarLsp");
      if (!currentConfig.get("enable") && client) {
        client.stop();
        client = undefined;
      }
    }
  });

  context.subscriptions.push(formatCommand, formatOnSave, configListener);
}

async function checkExecutableExists(command: string): Promise<boolean> {
  return new Promise((resolve) => {
    const child = spawn(command, ["--version"], {
      shell: process.platform === "win32",
      windowsHide: true,
    });

    child.on("error", () => resolve(false));
    child.on("close", (code) => resolve(code === 0));
  });
}

async function startLanguageServer(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration("solarLsp");
  const solarPath = config.get<string>("serverPath", "solar");
  const forgePath = config.get<string>("forgePath", "forge");
  const flychecks = config.get("flychecks");

  // Check if solar is available first
  let serverCommand: string;

  const solarExists = await checkExecutableExists(solarPath);

  if (solarExists) {
    console.log("Using solar lsp");
    serverCommand = solarPath;
  } else {
    console.log("Solar not found, checking for forge lsp...");
    const forgeExists = await checkExecutableExists(forgePath);

    if (forgeExists) {
      console.log("Using forge lsp as fallback");
      serverCommand = forgePath;
    } else {
      const errorMessage =
        "Neither solar nor forge are available. Please install one of them.";
      console.error(errorMessage);
      vscode.window.showErrorMessage(errorMessage);
      return;
    }
  }

  // Define server options
  const serverOptions: ServerOptions = {
    command: serverCommand,
    args: ["lsp"],
    transport: TransportKind.stdio,
  };

  // Define client options
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "solidity" }],
    initializationOptions: {
      forgePath,
      flychecks,
    },
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.sol"),
    },
  };

  // Create the language client and start it
  client = new LanguageClient(
    "solarLsp",
    "Solar LSP",
    serverOptions,
    clientOptions,
  );

  // Start the client. This will also launch the server
  client
    .start()
    .then(() => {
      const serverName = solarExists ? "Solar" : "Forge";
      console.log(`${serverName} LSP client started`);
      vscode.window.showInformationMessage(
        `${serverName} LSP started successfully`,
      );
    })
    .catch((error) => {
      console.error("Failed to start LSP client:", error);
      vscode.window.showErrorMessage(`Failed to start LSP: ${error.message}`);
    });

  // Add client to subscriptions so it gets disposed when extension is deactivated
  context.subscriptions.push(client);
}

async function formatDocument(
  document: vscode.TextDocument,
): Promise<vscode.TextEdit[]> {
  if (!serverSupportsDocumentFormatting()) {
    const edit = await formatDocumentWithForge(document);
    return edit ? [edit] : [];
  }

  const editorConfig = vscode.workspace.getConfiguration("editor", document.uri);
  const options: vscode.FormattingOptions = {
    tabSize: editorConfig.get<number>("tabSize", 4),
    insertSpaces: editorConfig.get<boolean>("insertSpaces", true),
  };
  const edits = await vscode.commands.executeCommand<vscode.TextEdit[]>(
    "vscode.executeFormatDocumentProvider",
    document.uri,
    options,
  );
  return edits ?? [];
}

function serverSupportsDocumentFormatting(): boolean {
  return (
    client?.state === State.Running &&
    Boolean(client.initializeResult?.capabilities.documentFormattingProvider)
  );
}

async function formatDocumentWithForge(
  document: vscode.TextDocument,
): Promise<vscode.TextEdit | undefined> {
  const config = vscode.workspace.getConfiguration("solarLsp");
  const forgePath = config.get<string>("forgePath", "forge");

  return new Promise((resolve) => {
    const forgeProcess = spawn(forgePath, ["fmt", "--raw", "-"], {
      stdio: ["pipe", "pipe", "pipe"],
      shell: process.platform === "win32",
      windowsHide: true,
    });

    let stdout = "";
    let stderr = "";

    forgeProcess.stdout.on("data", (data) => {
      stdout += data.toString();
    });

    forgeProcess.stderr.on("data", (data) => {
      stderr += data.toString();
    });

    forgeProcess.on("close", (code) => {
      if (code === 0) {
        const firstLine = document.lineAt(0);
        const lastLine = document.lineAt(document.lineCount - 1);
        const textRange = new vscode.Range(
          firstLine.range.start,
          lastLine.range.end,
        );

        resolve(new vscode.TextEdit(textRange, stdout));
      } else {
        console.error(`forge fmt failed with code ${code}: ${stderr}`);
        vscode.window.showErrorMessage(`Formatting failed: ${stderr}`);
        resolve(undefined);
      }
    });

    forgeProcess.on("error", (error) => {
      console.error("Failed to run forge fmt:", error);
      vscode.window.showErrorMessage(
        `Failed to run forge fmt: ${error.message}`,
      );
      resolve(undefined);
    });

    forgeProcess.stdin.write(document.getText());
    forgeProcess.stdin.end();
  });
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

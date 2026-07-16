import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
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

      await vscode.commands.executeCommand("editor.action.formatDocument");
    },
  );

  // Register format on save
  const formatOnSave = vscode.workspace.onWillSaveTextDocument((event) => {
    const currentConfig = vscode.workspace.getConfiguration("solarLsp");
    if (
      currentConfig.get("formatOnSave") &&
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
  let serverArgs: string[];

  const solarExists = await checkExecutableExists(solarPath);

  if (solarExists) {
    console.log("Using solar lsp");
    serverCommand = solarPath;
    serverArgs = ["lsp"];
  } else {
    console.log("Solar not found, checking for forge lsp...");
    const forgeExists = await checkExecutableExists(forgePath);

    if (forgeExists) {
      console.log("Using forge lsp as fallback");
      serverCommand = forgePath;
      serverArgs = ["lsp"];
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
    args: serverArgs,
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

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

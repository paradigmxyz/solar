import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ReferencesRequest,
  ServerOptions,
  State,
  TransportKind,
} from "vscode-languageclient/node";
import { spawn } from "child_process";

let client: LanguageClient | undefined;
let clientLifecycle: Promise<void> = Promise.resolve();

const restartSettings = [
  "solarLsp.enable",
  "solarLsp.codeLens.enable",
  "solarLsp.codeLens.selectors",
  "solarLsp.codeLens.references",
  "solarLsp.codeLens.inheritance",
];

export function activate(context: vscode.ExtensionContext) {
  const fileWatcher = vscode.workspace.createFileSystemWatcher("**/*.sol");
  context.subscriptions.push(fileWatcher);

  // Start the LSP server
  void restartLanguageServer(fileWatcher);

  // Register format document command
  const formatCommand = vscode.commands.registerCommand(
    "solarLsp.formatDocument",
    async () => {
      const currentConfig = vscode.workspace.getConfiguration("solarLsp");
      if (!currentConfig.get<boolean>("enable", true)) {
        return;
      }

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
      currentConfig.get<boolean>("enable", true) &&
      currentConfig.get<boolean>("formatOnSave", true) &&
      (!editorFormatOnSave || !serverSupportsDocumentFormatting()) &&
      event.document.languageId === "solidity"
    ) {
      event.waitUntil(formatDocument(event.document));
    }
  });

  const configListener = vscode.workspace.onDidChangeConfiguration((event) => {
    if (restartSettings.some((setting) => event.affectsConfiguration(setting))) {
      void restartLanguageServer(fileWatcher);
    }
  });

  const copySelectorCommand = vscode.commands.registerCommand(
    "solar.copySelector",
    copySelector,
  );
  const showReferencesCommand = vscode.commands.registerCommand(
    "solar.showReferences",
    showReferences,
  );
  const showTypeHierarchyCommand = vscode.commands.registerCommand(
    "solar.showTypeHierarchy",
    showTypeHierarchy,
  );

  context.subscriptions.push(
    formatCommand,
    formatOnSave,
    configListener,
    copySelectorCommand,
    showReferencesCommand,
    showTypeHierarchyCommand,
  );
}

function restartLanguageServer(
  fileWatcher: vscode.FileSystemWatcher,
): Promise<void> {
  clientLifecycle = clientLifecycle
    .then(async () => {
      await stopLanguageServer();

      const config = vscode.workspace.getConfiguration("solarLsp");
      if (config.get<boolean>("enable", true)) {
        await startLanguageServer(fileWatcher);
      }
    })
    .catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      console.error("Failed to restart LSP client:", error);
      vscode.window.showErrorMessage(`Failed to restart LSP: ${message}`);
    });
  return clientLifecycle;
}

async function stopLanguageServer(): Promise<void> {
  const currentClient = client;
  if (!currentClient) {
    return;
  }

  if (currentClient.state === State.Running) {
    await currentClient.stop();
  }

  if (client === currentClient) {
    client = undefined;
  }
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

async function startLanguageServer(fileWatcher: vscode.FileSystemWatcher) {
  const config = vscode.workspace.getConfiguration("solarLsp");
  const solarPath = config.get<string>("serverPath", "solar");
  const forgePath = config.get<string>("forgePath", "forge");
  const flychecks = config.get("flychecks");
  const codeLens = {
    enable: config.get<boolean>("codeLens.enable", true),
    selectors: config.get<boolean>("codeLens.selectors", true),
    references: config.get<boolean>("codeLens.references", true),
    inheritance: config.get<boolean>("codeLens.inheritance", true),
    clientCommands: true,
  };

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
      codeLens,
    },
    synchronize: {
      fileEvents: fileWatcher,
    },
  };

  // Create the language client and start it
  const nextClient = new LanguageClient(
    "solarLsp",
    "Solar LSP",
    serverOptions,
    clientOptions,
  );
  client = nextClient;

  // Start the client. This will also launch the server
  try {
    await nextClient.start();
    const serverName = solarExists ? "Solar" : "Forge";
    console.log(`${serverName} LSP client started`);
    vscode.window.showInformationMessage(
      `${serverName} LSP started successfully`,
    );
  } catch (error) {
    try {
      await nextClient.dispose();
    } catch {
      // Failed starts can also reject shutdown after scheduling process cleanup.
    }
    if (client === nextClient) {
      client = undefined;
    }
    const message = error instanceof Error ? error.message : String(error);
    console.error("Failed to start LSP client:", error);
    vscode.window.showErrorMessage(`Failed to start LSP: ${message}`);
  }
}

async function copySelector(selector: unknown): Promise<void> {
  if (typeof selector !== "string" || selector.length === 0) {
    return;
  }
  await vscode.env.clipboard.writeText(selector);
}

async function showReferences(argument: unknown): Promise<void> {
  const location = parseCodeLensLocation(argument);
  const runningClient = client;
  if (!location || !runningClient || runningClient.state !== State.Running) {
    return;
  }

  try {
    const result = await runningClient.sendRequest(ReferencesRequest.type, {
      textDocument: { uri: location.uri.toString() },
      position: {
        line: location.position.line,
        character: location.position.character,
      },
      context: { includeDeclaration: false },
    });
    const references = await runningClient.protocol2CodeConverter.asReferences(
      result ?? [],
    );
    await vscode.commands.executeCommand(
      "editor.action.showReferences",
      location.uri,
      location.position,
      references,
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error("Failed to show references:", error);
    vscode.window.showErrorMessage(`Failed to show references: ${message}`);
  }
}

async function showTypeHierarchy(argument: unknown): Promise<void> {
  const location = parseCodeLensLocation(argument);
  if (!location || !argument || typeof argument !== "object") {
    return;
  }

  const direction = (argument as { direction?: unknown }).direction;
  if (direction !== "supertypes" && direction !== "subtypes") {
    return;
  }

  try {
    const range = new vscode.Range(location.position, location.position);
    const editor = await vscode.window.showTextDocument(location.uri, {
      selection: range,
    });
    editor.revealRange(
      range,
      vscode.TextEditorRevealType.InCenterIfOutsideViewport,
    );
    await vscode.commands.executeCommand("editor.showTypeHierarchy");
    await vscode.commands.executeCommand(
      direction === "supertypes"
        ? "editor.showSupertypes"
        : "editor.showSubtypes",
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error("Failed to show type hierarchy:", error);
    vscode.window.showErrorMessage(`Failed to show type hierarchy: ${message}`);
  }
}

function parseCodeLensLocation(
  argument: unknown,
): { uri: vscode.Uri; position: vscode.Position } | undefined {
  if (!argument || typeof argument !== "object") {
    return undefined;
  }

  const candidate = argument as {
    uri?: unknown;
    position?: { line?: unknown; character?: unknown };
  };
  if (
    typeof candidate.uri !== "string" ||
    !candidate.position ||
    typeof candidate.position.line !== "number" ||
    typeof candidate.position.character !== "number" ||
    !Number.isInteger(candidate.position.line) ||
    !Number.isInteger(candidate.position.character) ||
    candidate.position.line < 0 ||
    candidate.position.character < 0
  ) {
    return undefined;
  }

  return {
    uri: vscode.Uri.parse(candidate.uri),
    position: new vscode.Position(
      candidate.position.line,
      candidate.position.character,
    ),
  };
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
  return clientLifecycle.then(stopLanguageServer);
}

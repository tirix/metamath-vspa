import { commands, window, workspace, ExtensionContext, TextDocument, EndOfLine } from 'vscode';

import {
	LanguageClient,
	LanguageClientOptions,
	ServerOptions,
	ErrorAction,
	CloseAction
} from 'vscode-languageclient';

let client: LanguageClient;

function startClient() {
	let config = workspace.getConfiguration('metamath');
	let mmLspPath: string = config.get('executablePath') || 'mm-lsp-server';

	// If the extension is launched in debug mode then the debug server options are used
	// Otherwise the run options are used
	let serverOptions: ServerOptions = {
		run: { command: mmLspPath, args: [] },
		debug: { command: mmLspPath, args: ['--debug'] }
	};

	// Options to control the language client
	let clientOptions: LanguageClientOptions = {
		// Register the server for MM files
		documentSelector: [{ scheme: 'file', language: 'metamath' }, { scheme: 'file', language: 'metamath-proof' }],
		initializationOptions: { extraCapabilities: { goalView: true } }
	};

	// Create the language client and start the client.
	client = new LanguageClient(
		'metamath', 'Metamath Server', serverOptions, clientOptions);

	// Start the client. This will also launch the server
	client.start();
}

export function activate(context: ExtensionContext) {
	console.log('Launching client!');
	startClient();

	console.log('"Subscribing commands!');
	context.subscriptions.push(
		commands.registerCommand('metamath.unify', () => {
			// Display a message box to the user
			window.showInformationMessage('Hello World from Metamath!');
		}),
		commands.registerCommand('metamath.shutdownServer',
		  () => client.stop().then(() => {}, () => {})),
		commands.registerCommand('metamath.restartServer',
			() => client.stop().then(startClient, startClient))
	);
	console.log('"Metamath" extension is now active!');
}

export function deactivate(): Thenable<void> | undefined {
	if (client) {
		console.log('Stopping client!');
		return client.stop();
	}
	console.log('"Metamath" extension is now inactive!');
	return undefined;
}

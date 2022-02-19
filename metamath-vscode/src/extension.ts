import { commands, window, workspace, ExtensionContext, Range, Position, Selection, EndOfLine, CodeActionKind, Uri } from 'vscode';
import * as fs from 'fs'; 
import {
	LanguageClient,
	LanguageClientOptions,
	ServerOptions,
	ErrorAction,
	CloseAction
} from 'vscode-languageclient';
import {
	TextDocumentIdentifier,
} from 'vscode-languageserver-types';
import {
	TextDocumentPositionParams
} from 'vscode-languageserver-protocol';
import {
	RequestType,
	NotificationType
} from 'vscode-jsonrpc';

let client: LanguageClient;

// Using our own parameter type does not seem to work
// interface ShowProofParams {
//	textDocument: TextDocumentIdentifier;
//	range: Range;
// }

namespace ShowProofRequest {
	export const type = new RequestType<string, string, void>('metamath/showProof');
}

function startClient() {
	// global config
	let config = workspace.getConfiguration('metamath');
	// local config, like main MM file
	let configFilePath = './.metamath.json';
	if (fs.existsSync(configFilePath)) {
		config.push(...JSON.parse(fs.readFileSync(configFilePath, 'utf-8')));
	}

	let mmLspPath: string = config.get('executablePath') || 'mm-lsp-server';
	let mainFilePath: string = config.get('mainFilePath') || 'set.mm';
	let jobs: string = config.get('jobs') || '1';

	// If the extension is launched in debug mode then the debug server options are used
	// Otherwise the run options are used
	let serverOptions: ServerOptions = {
		run: { command: mmLspPath, args: [ '--jobs', jobs, mainFilePath ] },
		debug: { command: mmLspPath, args: [ '--jobs', jobs, mainFilePath ] }, //['--debug', '--jobs', jobs, mainFilePath] }
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
		// For File search, see https://github.com/microsoft/vscode/issues/73524
		// For Text search, see https://github.com/microsoft/vscode/issues/59921
		commands.registerCommand('metamath.showProof', showProof),
		commands.registerCommand('metamath.unify', unify),
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

function showProof() {
	window.showInformationMessage('Show proof!');
	const editor = window.activeTextEditor;
	let selectionRange: Range;
	if(!editor) {
		return;
	}

	if (!editor.selection.isEmpty) {
		selectionRange = new Range(
			editor.selection.start,
			editor.selection.end
		);
	} else {
		selectionRange = editor.document.lineAt(editor.selection.start.line).range;
	}
	let label = editor.document.getText(selectionRange);
	// 
	// let params: ShowProofParams = {
	// 	textDocument: TextDocumentIdentifier.create(editor.document.uri.toString()),
	// 	range: selectionRange
	// };
	client.sendRequest(ShowProofRequest.type, label).then(async (content) => {
		// Open a new document with the given MMP content
		const doc = await workspace.openTextDocument({
			language: 'metamath-proof',
			content: content
		});
		return await window.showTextDocument(doc);
	});
}

function unify() {
	// Display a message box to the user
	window.showInformationMessage('Hello World from Metamath!');
	//			editor.edit((editBuilder) => {
	//				editBuilder.replace(range, result);
	//			});
}
# metamath-vspa
A Visual Studio extension and LSP server for Metamath

## Status

The server is still in an early experimental state. It is usable but many advanced features have not yet been implemented.

## How-to

### Installation

Install [Visual Studio Code](https://code.visualstudio.com/), and install [rust](https://www.rust-lang.org/tools/install) if not already on your system.
Then, intall the Metamath LSP server. Until `metamath-knife` and `metamath-vspa` are delivered as crate.io crates, this has to be done manually:
```
git clone https://github.com/david-a-wheeler/metamath-knife.git
git clone https://github.com/tirix/metamath-vspa.git
cargo install --path metamath-vspa/metamath-lsp
```
This shall compile and install the LSP server `mm-lsp-server` binary, accessible from your default path.

Ultimately, the VSCode extension is also meant to be delivered on the Visual Studio Code marketplace. Until then, you can open it from Visual Studio Code itself:
* Open the directory `metamath-vspa/metamath-vscode`
* Choose 'Run/Start Debugging' from the menu or hit the corresponding shortcut (F5)

## Features

* Hovering over a label provides the statement information (hypotheses, assertion, associated comment)
* The "Go to definition" command, when performed on a label, leads to the corresponding statement's definition.

## Acknowledgements

- This server is based on Mario Carneiro's LSP server for MM0.
- Its core functions are provided by the [metamath-knife](https://github.com/david-a-wheeler/metamath-knife) library.

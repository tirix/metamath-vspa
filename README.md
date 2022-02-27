[![Build Status](https://github.com/tirix/metamath-vspa/actions/workflows/ci.yml/badge.svg)](https://github.com/tirix/metamath-vspa/actions?query=workflow%3Aci)

# metamath-vspa
A Visual Studio extension and LSP server for Metamath

## Status

The server is still in an early experimental state. It is usable but many advanced features have not yet been implemented.

## How-to

### Installation

Install [Visual Studio Code](https://code.visualstudio.com/), and install [rust](https://www.rust-lang.org/tools/install) if not already on your system.
Then, install the Metamath LSP server. Until `metamath-knife` and `metamath-vspa` are delivered as crate.io crates, this has to be done manually:
```
git clone https://github.com/david-a-wheeler/metamath-knife.git
git clone https://github.com/tirix/metamath-vspa.git
cargo install --path metamath-vspa/metamath-lsp
```
This shall compile and install the LSP server `mm-lsp-server` binary, accessible from your default path.

You can then install the Visual Studio Code extension. There are several possible ways for that:
- in a web browser, from [the extension's Visual Studio Code Marketplace web page](https://marketplace.visualstudio.com/items?itemName=tirix.metamath), press the green "install" button,
- in Visual Studio Code, from the View/Extensions menu, search for *Metamath* "A Metamath proof assistant" (`tirix.metamath`), and press the blue "install" button.
- in Visual Studio Code, use Quick Open (Ctrl-P on Windows/Linux, Cmd-P on MacOS), paste `ext install tirix.metamath` in the box and hit Enter (Return). 

## Contributing / Development

It also possible to launch the extension from the source, using Visual Studio Code itself, for example if you wisth to modify it and contribute to the project:
* Open the directory `metamath-vspa/metamath-vscode`
* Install [node.js and npm](https://nodejs.org/en/download/)
* Launch `npm install` to install pre-requisites
* Choose 'Run/Start Debugging' from the menu or hit the corresponding shortcut (F5)

## Features

* Hovering over a label provides the statement information (hypotheses, assertion, associated comment),
* The "Go to definition" command, when performed on a label, leads to the corresponding statement's definition,
* The "Show Proof" context menu opens a theorem's proof in a new editor tab,
* Diagnostics for the opened Metamath data

Preview:

![mm-vscode-1](https://user-images.githubusercontent.com/5831830/153800753-80e6af30-7a5e-4154-addb-39bd3ff1ae6f.gif)

## Acknowledgements

- This server is based on Mario Carneiro's LSP server for MM0.
- Its core functions are provided by the [metamath-knife](https://github.com/david-a-wheeler/metamath-knife) library, initially by Stefan O'Rear.

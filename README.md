# metamath-vspa
A Visual Studio extension and LSP server for Metamath

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


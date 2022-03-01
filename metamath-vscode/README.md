# Metamath for Visual Studio Code

This extension provides language support for the Metamath, through the [Metamath LSP server](https://github.com/tirix/metamath-vspa/tree/master/metamath-lsp). 

## Features

This extension supports:

* jump to definition
* axioms and theorem documentation when hovering over their labels
* proof display 
* diagnostics

## Requirements

This extension requires the [Metamath LSP server](https://github.com/tirix/metamath-vspa/tree/master/metamath-lsp). See corresponding [installation instructions](https://github.com/tirix/metamath-vspa#installation).

## Extension Settings

This extension currently only provides the `metamath.executablePath` option in VSCode's configuration settings, which allows to specify the . You can find the settings under File > Preferences > Settings.

## Workspace Settings

When starting up, the extension will search for a file named `.metamath.json` at the root of the workspace directory, with parameters corresponding to the specific database to be loaded and used.

Here is a sample Metamath workspace onfiguration file:
```json
{
    "jobs": 8,
    "mainFile": "set.mm"
}
```

This file contains the following parameters:
* `jobs` : Number of threads to use for parsing the database
* `mainFile`: Main database file in this directory. That file may include other files through the `$[` include `$]` syntax. 

## Known Issues

Calling out known issues can help limit users opening duplicate issues against your extension.

## Release Notes

### 0.0.2

* Basic syntax highlighting for Metamath files and for Proof files
* Fix dependency issue

### 0.0.1

Initial release, 
* LSP connection (provides definitions, hover, diagnostics)
* Show proof functionality

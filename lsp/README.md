# FOLPlan language server

`folplan-lsp` supplies syntax and model validation diagnostics plus keyword
completion for `.fol` files. It uses the same parser and validator as
`folplan check`. PDDL remains a two-file import format and is outside this
single-document language server.

Install from the repository root:

```sh
just installation
```

This installs both CLI and language server in `~/.local/bin` and copies the
Neovim plugin. Restart Neovim. The plugin uses Neovim's built-in LSP client
and sets the `folplan` filetype for `.fol`. No Mason package is needed. Re-run
`just installation` after changing the parser or validator.

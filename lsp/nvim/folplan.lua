return {
  {
    "neovim/nvim-lspconfig",
    init = function()
      vim.filetype.add({ extension = { fol = "folplan" } })
      vim.lsp.config("folplan", {
        cmd = { vim.fn.expand("~/.local/bin/folplan-lsp") },
        filetypes = { "folplan" },
        root_markers = { ".git" },
      })
      vim.lsp.enable("folplan")
    end,
  },
}

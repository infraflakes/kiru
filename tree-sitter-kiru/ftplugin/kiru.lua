-- Start tree-sitter highlighting for kiru buffers. Neovim looks up the
-- parser in `parser/kiru.so` and the queries in `queries/kiru/`, both under
-- this runtimepath entry.
if vim.treesitter and vim.treesitter.start then
  vim.treesitter.start(0, 'kiru')
end

vim.bo.commentstring = '# %s'

#!/usr/bin/env bash
set -euo pipefail

root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
out_jsonl="${RS_DIAGNOSTICS_JSONL:-$root/rs-diagnostics.jsonl}"
out_txt="${RS_DIAGNOSTICS_TXT:-$root/rs-diagnostics.txt}"
timeout_ms="${RS_DIAGNOSTICS_TIMEOUT_MS:-2000}"

lua_script=$(mktemp "${TMPDIR:-/tmp}/rs-diagnostics.XXXXXX.lua")
trap 'rm -f "$lua_script"' EXIT

cat >"$lua_script" <<'LUA'
local root = vim.env.RS_DIAGNOSTICS_ROOT
local out_jsonl = vim.env.RS_DIAGNOSTICS_JSONL
local out_txt = vim.env.RS_DIAGNOSTICS_TXT
local timeout_ms = tonumber(vim.env.RS_DIAGNOSTICS_TIMEOUT_MS or "2000")

local files
if vim.fn.executable("fd") == 1 then
  files = vim.fn.systemlist("cd " .. vim.fn.shellescape(root) .. " && fd -e rs -E target")
else
  files = vim.fn.systemlist("git -C " .. vim.fn.shellescape(root) .. " ls-files '*.rs'")
end
local json_lines, text_lines = {}, {}
local total, index, client = 0, 0, nil

local function severity_name(n)
  return ({ [1] = "ERROR", [2] = "WARN", [3] = "INFO", [4] = "HINT" })[n] or tostring(n)
end

local function include(d)
  return d.severity and d.severity <= vim.diagnostic.severity.INFO
end

local function append_diag(file, d)
  if not include(d) then return end
  total = total + 1
  local item = {
    file = file,
    line = d.range.start.line + 1,
    column = d.range.start.character + 1,
    severity = severity_name(d.severity),
    source = d.source,
    code = d.code,
    message = d.message,
  }
  table.insert(json_lines, vim.json.encode(item))
  table.insert(text_lines, string.format(
    "%s:%d:%d: %s: %s[%s] %s",
    item.file, item.line, item.column, item.severity,
    item.source or "", tostring(item.code or ""),
    (item.message or ""):gsub("\n", " ")
  ))
end

local function finish()
  table.insert(text_lines, 1, string.format("TOTAL %d", total))
  vim.fn.writefile(json_lines, out_jsonl)
  vim.fn.writefile(text_lines, out_txt)
  io.stderr:write(string.format("done: files=%d diagnostics=%d\n", #files, total))
  vim.cmd("qa!")
end

local function next_file()
  index = index + 1
  if index > #files then return finish() end
  local file = files[index]
  vim.cmd("silent keepalt edit " .. vim.fn.fnameescape(root .. "/" .. file))
  local bufnr = vim.api.nvim_get_current_buf()
  local done = false

  local function continue()
    if done then return end
    done = true
    if index % 25 == 0 then
      io.stderr:write(string.format("processed %d/%d diagnostics=%d\n", index, #files, total))
    end
    vim.schedule(next_file)
  end

  vim.defer_fn(continue, timeout_ms)
  local params = { textDocument = vim.lsp.util.make_text_document_params(bufnr) }
  client:request("textDocument/diagnostic", params, function(err, res)
    if err then
      table.insert(text_lines, string.format("%s: request error: %s", file, vim.inspect(err):gsub("\n", " ")))
    elseif res and res.items then
      for _, d in ipairs(res.items) do append_diag(file, d) end
    end
    continue()
  end, bufnr)
end

vim.cmd("silent edit " .. vim.fn.fnameescape(root .. "/" .. (files[1] or "src/main.rs")))
vim.wait(120000, function() return #vim.lsp.get_clients({ name = "rust-analyzer" }) > 0 end, 200)
client = vim.lsp.get_clients({ name = "rust-analyzer" })[1]
if not client then
  vim.fn.writefile({ "rust-analyzer client not found" }, out_txt)
  vim.cmd("cquit 1")
end

vim.defer_fn(next_file, 1000)
LUA

RS_DIAGNOSTICS_ROOT="$root" \
RS_DIAGNOSTICS_JSONL="$out_jsonl" \
RS_DIAGNOSTICS_TXT="$out_txt" \
RS_DIAGNOSTICS_TIMEOUT_MS="$timeout_ms" \
nvim --headless "$root/src/main.rs" -S "$lua_script"

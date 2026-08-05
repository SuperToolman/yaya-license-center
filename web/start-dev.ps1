$ErrorActionPreference = "Stop"

$nextBinary = Join-Path $PSScriptRoot "node_modules\next\dist\bin\next"
if (-not (Test-Path $nextBinary)) {
  throw "缺少前端依赖。请先在此目录执行 pnpm install。"
}

node $nextBinary dev --port 3001

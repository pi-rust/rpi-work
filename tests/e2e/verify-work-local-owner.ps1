param([string]$RpcFixture = '')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Push-Location $repo
$previous = @{}
try {
    if (-not $RpcFixture) {
        $metadata = (& cargo metadata --locked --no-deps --format-version 1) | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve the Cargo target directory.' }
        $suffix = if ($env:OS -eq 'Windows_NT') { '.exe' } else { '' }
        $RpcFixture = Join-Path $metadata.target_directory "debug/work-server-rpc-fixture$suffix"
    }
    if (-not (Test-Path -LiteralPath $RpcFixture -PathType Leaf)) { throw "Build the RPC fixture first: $RpcFixture" }
    $root = Join-Path ([IO.Path]::GetTempPath()) ('work-local-owner-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path (Join-Path $root 'agent') -Force | Out-Null
    $variables = @{
        RPI_TEST_ISOLATED='1'; RPI_WORK_DATA_DIR=(Join-Path $root 'node');
        RPI_CODING_AGENT_DIR=(Join-Path $root 'agent');
        RPI_WORK_RPI_BIN=(Resolve-Path -LiteralPath $RpcFixture).Path;
        RPI_WORK_NODE_EXTENSION=(Join-Path $root 'must-not-load.dll');
        RPI_WORK_NODE_RPI_BIN=(Join-Path $root 'must-not-run.exe');
        RPI_WORK_HTTP_BIND='127.0.0.1'; RPI_WORK_HTTP_PORT='0';
        RPI_WORK_HTTP_PASSWORD='isolated-owner-test'; RPI_HTTP_PASSWORD=$null;
        RPI_WORK_HTTP_PUBLIC_URL=$null; RPI_WORK_HTTP_ALLOWED_HOSTS=$null;
        RPI_WORK_HTTP_ASSETS=$null; RPI_WORK_HTTP_WEB_PORT=$null;
        RPI_NODE_CONTROL_TOKEN=$null; RPI_NODE_CONTROL_STDIN=$null;
    }
    foreach ($name in $variables.Keys) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        if ($null -eq $variables[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $variables[$name], 'Process') }
    }
    & cargo test --locked -p work-host --test local_owner -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'Work local owner verification failed.' }
} finally {
    foreach ($name in $previous.Keys) {
        if ($null -eq $previous[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    }
    Pop-Location
}

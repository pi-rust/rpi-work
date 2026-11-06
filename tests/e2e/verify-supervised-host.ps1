param(
    [Parameter(Mandatory)][string]$RpiBin,
    [string]$Extension = '',
    [string]$RpcFixture = '',
    [switch]$UseSelectedRuntime
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
if (-not $Extension) { $Extension = Join-Path $repo 'target/debug/rpi_http_extension.dll' }
if (-not $RpcFixture) { $RpcFixture = Join-Path $repo 'target/debug/work-server-rpc-fixture.exe' }
foreach ($required in @($RpiBin,$Extension,$RpcFixture)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Build required: $required" }
}
$root = Join-Path ([IO.Path]::GetTempPath()) ('rpi-supervised-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$variables = @{
    RPI_TEST_ISOLATED='1'; RPI_WORK_DATA_DIR=(Join-Path $root 'node');
    RPI_CODING_AGENT_DIR=(Join-Path $root 'agent'); RPI_WORK_COMPONENTS_DIR=(Join-Path $root 'components');
    RPI_WORK_NODE_RPI_BIN=(Resolve-Path -LiteralPath $RpiBin).Path;
    RPI_WORK_NODE_EXTENSION=(Resolve-Path -LiteralPath $Extension).Path;
    RPI_WORK_RPI_BIN=(Resolve-Path -LiteralPath $RpcFixture).Path;
    RPI_WORK_HTTP_BIND='127.0.0.1'; RPI_WORK_HTTP_PORT='0';
    RPI_WORK_HTTP_PUBLIC_URL=$null; RPI_WORK_HTTP_ALLOWED_HOSTS=$null;
    RPI_WORK_HTTP_PASSWORD='isolated-supervision-only'; RPI_HTTP_PASSWORD=$null;
    RPI_WORK_HTTP_WEB_PORT=$null; RPI_WORK_HTTP_ASSETS=$null;
    RPI_NODE_CONTROL_TOKEN=$null; RPI_NODE_CONTROL_STDIN=$null;
}
# The dedicated service bootstrap is intentionally independent of model auth.
Get-ChildItem Env: | Where-Object { $_.Name -match 'API_KEY|AUTH_TOKEN|ACCESS_TOKEN|BEARER_TOKEN' } | ForEach-Object { $variables[$_.Name]=$null }
$previous=@{}
Push-Location $repo
try {
    if ($UseSelectedRuntime) {
        $target='x86_64-pc-windows-msvc'
        $descriptor=@{entryFlag='http';artifacts=@{ $target=@{url='https://fixture.invalid/runtime.dll';sha256=(Get-FileHash -LiteralPath $Extension -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $Extension).Length} }}
        $descriptorFile=Join-Path $root 'runtime-descriptor.json'
        [IO.File]::WriteAllText($descriptorFile,($descriptor | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
        $document=& cargo run --quiet --locked -p rpi-http-component --bin export-component -- $descriptorFile
        if ($LASTEXITCODE -ne 0) { throw 'Native selection fixture export failed.' }
        $bundle=($document -join "`n") | ConvertFrom-Json
        $packageDirectory=Join-Path $variables.RPI_WORK_COMPONENTS_DIR "rpi-http/$($bundle.sha256)"
        New-Item -ItemType Directory -Path $packageDirectory -Force | Out-Null
        Copy-Item -LiteralPath $Extension -Destination (Join-Path $packageDirectory 'runtime.dll')
        [IO.File]::WriteAllText((Join-Path $packageDirectory 'component.json'),($document -join "`n"),[Text.UTF8Encoding]::new($false))
        $selection=@{id=$bundle.manifest.id;version=$bundle.manifest.version;bundleSha256=$bundle.sha256;artifactSha256=$descriptor.artifacts[$target].sha256;target=$target;entryFlag='http'}
        [IO.File]::WriteAllText((Join-Path $variables.RPI_WORK_COMPONENTS_DIR 'selection.json'),($selection | ConvertTo-Json),[Text.UTF8Encoding]::new($false))
        $variables.RPI_WORK_NODE_EXTENSION=$null
        $variables.RPI_TEST_USE_SELECTED_RUNTIME='1'
    } else { $variables.RPI_TEST_USE_SELECTED_RUNTIME=$null }
    foreach ($name in $variables.Keys) {
        $previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')
        if ($null -eq $variables[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name,$variables[$name],'Process') }
    }
    & cargo test --locked -p work-host --test native_extension -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'Supervised native host verification failed.' }
} finally {
    foreach ($name in $previous.Keys) {
        if ($null -eq $previous[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
    }
    Pop-Location
}

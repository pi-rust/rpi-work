param([Parameter(Mandatory=$true)][ValidatePattern('^[a-z0-9][a-z0-9-]{0,63}$')][string]$Name)
$ErrorActionPreference='Stop'
$repoRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$checkpoint=Join-Path $repoRoot "target/parity-checkpoints/$Name"
if(Test-Path -LiteralPath $checkpoint){throw "Checkpoint already exists: $checkpoint"}
$paths=@((Join-Path $repoRoot 'Cargo.toml'),(Join-Path $repoRoot 'Cargo.lock'))+@(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'apps'),(Join-Path $repoRoot 'crates'),(Join-Path $repoRoot 'assets') -Recurse -File | Where-Object {$_.Extension -in '.rs','.css','.toml','.js','.txt'} | ForEach-Object {$_.FullName})
New-Item -ItemType Directory -Path $checkpoint | Out-Null
$hashes=[ordered]@{}
foreach($source in ($paths | Sort-Object)){
 $relative=$source.Substring($repoRoot.Length+1).Replace('\','/')
 $hash=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
 $destination=Join-Path $checkpoint $relative
 New-Item -ItemType Directory -Force -Path (Split-Path $destination -Parent) | Out-Null
 Copy-Item -LiteralPath $source -Destination $destination
 if((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $hash -or (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $hash){throw "Source changed during checkpoint: $relative"}
 $hashes[$relative]=$hash
}
[ordered]@{createdAt=[DateTime]::UtcNow.ToString('o');sourceHashes=$hashes} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $checkpoint 'checkpoint.json') -Encoding utf8
Write-Output "Saved $($hashes.Count) source files: $checkpoint"

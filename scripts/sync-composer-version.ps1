# Sync php-driver/composer.json "version" from [workspace.package].version in Cargo.toml.
# Usage: powershell -File scripts/sync-composer-version.ps1

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$cargoToml = Join-Path $root "Cargo.toml"
$composerJson = Join-Path $root "php-driver\composer.json"

$inWorkspacePackage = $false
$version = $null
foreach ($line in Get-Content $cargoToml) {
    if ($line -match '^\[workspace\.package\]') {
        $inWorkspacePackage = $true
        continue
    }
    if ($inWorkspacePackage -and $line -match '^\[') {
        break
    }
    if ($inWorkspacePackage -and $line -match '^version = "([^"]+)"') {
        $version = $Matches[1]
        break
    }
}

if (-not $version) {
    Write-Error "Could not read [workspace.package].version from $cargoToml"
}

$json = Get-Content $composerJson -Raw
if ($json -match '"version"\s*:\s*"[^"]*"') {
    $json = $json -replace '"version"\s*:\s*"[^"]*"', "`"version`": `"$version`""
} else {
    $json = $json -replace '("name"\s*:\s*"nusa/octane",)', "`$1`n    `"version`": `"$version`","
}

Set-Content -Path $composerJson -Value $json.TrimEnd() -NoNewline
Add-Content -Path $composerJson -Value ""
Write-Host "php-driver/composer.json version -> $version"

# Builds Kel from this repository and installs it, as the palette's "Install Kel
# on this computer" does with a downloaded kel.exe (both run setup\kel.ps1):
# %LOCALAPPDATA%\Programs\Kel, the Start menu, PATH, "Open in Kel" in Explorer.
# Run it again after changes; it works while Kel is open.
#
#   .\install.ps1           build and install
#   .\install.ps1 -NoMenu   the same, without the menu (it's taken away if there)

param([switch]$NoMenu)

Set-Location $PSScriptRoot

# Not `$ErrorActionPreference = 'Stop'`: cargo writes its progress to stderr, and
# PowerShell 5 can take that for an error. Its exit code says if it failed.
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& "$PSScriptRoot\setup\kel.ps1" -From target\release\kel.exe -NoMenu:$NoMenu
if ($LASTEXITCODE) { exit $LASTEXITCODE }

# This script used to put kel.exe in cargo's folder, ~\.cargo\bin: that one
# would still answer `kel`, old. It steps aside the same way (it may be running).
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$HOME\.cargo" }
$old = Join-Path $cargoHome 'bin\kel.exe'
if (Test-Path $old) {
    $away = Join-Path $env:TEMP ('kel-{0:yyyyMMdd-HHmmss}.old' -f (Get-Date))
    Move-Item $old $away -ErrorAction SilentlyContinue
    Write-Host "[ok] the old kel.exe in $cargoHome\bin is gone (Kel's home is the one above now)"
}

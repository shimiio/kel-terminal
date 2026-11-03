# Builds Kel and puts it on the PATH as kel.exe, so `kel` and `kel <project>` work
# from any shell. Run it again after changes; it works while Kel is open.
#
#   .\install.ps1           build, install, and "Open in Kel" in Explorer's menu
#   .\install.ps1 -NoMenu   the same, without the menu (it's taken away if there)
#
# Where to: cargo's folder for programs, ~\.cargo\bin. Installing Rust put that
# folder on the PATH (that's how `cargo` itself is found), so there's nothing to add.

param([switch]$NoMenu)

Set-Location $PSScriptRoot

# "Open in Kel" on a folder in Explorer, and on the empty space inside an open
# one: it runs `kel "<folder>"` (a tab there, or the project of that folder).
# For you only (HKCU), so no admin. On Windows 11 it's under "Show more options"
# (or Shift+right-click): the short new menu takes only apps installed as packages.
# >>> menu
function Set-ExplorerMenu([string]$exe, [string]$key = 'kel', [switch]$Remove) {
    foreach ($where in 'Directory', 'Directory\Background') {
        $path = "HKCU:\Software\Classes\$where\shell\$key"
        if ($Remove) {
            Remove-Item $path -Recurse -ErrorAction SilentlyContinue
            continue
        }
        New-Item "$path\command" -Force | Out-Null
        Set-Item $path 'Open in Kel'                  # the menu's line
        Set-ItemProperty $path 'Icon' "$exe,0"         # Kel's icon, from the exe
        Set-Item "$path\command" "`"$exe`" `"%V`""      # %V: the folder
    }
}
# <<< menu

# Not `$ErrorActionPreference = 'Stop'`: cargo writes its progress to stderr, and
# PowerShell 5 can take that for an error. Its exit code says if it failed.
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$HOME\.cargo" }
$bin = Join-Path $cargoHome 'bin'
$exe = Join-Path $bin 'kel.exe'

# Windows doesn't let a running exe be written over (every open Kel runs this one),
# but it lets it be renamed. So the old one steps aside, then goes if no Kel runs
# it; one that still runs goes at a later install.
if (Test-Path $exe) {
    Rename-Item $exe ('kel-{0:yyyyMMdd-HHmmss}.old' -f (Get-Date)) -ErrorAction Stop
}
Copy-Item target\release\kel.exe $exe -ErrorAction Stop
Get-ChildItem $bin -Filter 'kel-*.old' | Remove-Item -ErrorAction SilentlyContinue
"installed: $exe"

if ($NoMenu) {
    Set-ExplorerMenu $exe -Remove
    "no ""Open in Kel"" in Explorer's menu"
} else {
    Set-ExplorerMenu $exe
    """Open in Kel"" in Explorer's menu, on a folder (Windows 11: Show more options)"
}

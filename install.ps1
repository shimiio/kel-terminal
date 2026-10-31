# Builds Kel and puts it on the PATH as kel.exe, so `kel` and `kel <project>` work
# from any shell. Run it again after changes; it works while Kel is open.
#
#   .\install.ps1
#
# Where to: cargo's folder for programs, ~\.cargo\bin. Installing Rust put that
# folder on the PATH (that's how `cargo` itself is found), so there's nothing to add.

Set-Location $PSScriptRoot

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

# Installs Kel for you, like any program, without admin rights:
#   - kel.exe in %LOCALAPPDATA%\Programs\Kel (where per-user programs go)
#   - Kel in the Start menu
#   - that folder on your PATH: `kel` and `kel <project>` in any new shell
#   - "Open in Kel" in Explorer's menu on a folder (Windows 11: Show more options)
#
# Kel runs it from its command palette (Install Kel on this computer), with the
# kel.exe that runs as -From. Run again with a newer kel.exe: that's an update.
#
#   kel.ps1 -From <kel.exe>           install it
#   kel.ps1 -From <kel.exe> -NoMenu   the same, without the menu (it's taken away if there)
#   kel.ps1 -Uninstall                take all of it away again (your config stays)

param([string]$From, [switch]$NoMenu, [switch]$Uninstall)

$dir = Join-Path $env:LOCALAPPDATA 'Programs\Kel'
$exe = Join-Path $dir 'kel.exe'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Kel.lnk'

# "Open in Kel" on a folder in Explorer, and on the empty space inside an open
# one: it runs `kel "<folder>"` (a tab there, or the project of that folder).
# For you only (HKCU), so no admin. On Windows 11 it's under "Show more options"
# (or Shift+right-click): the short new menu takes only apps installed as packages.
function Set-ExplorerMenu([string]$exe, [switch]$Remove) {
    foreach ($where in 'Directory', 'Directory\Background') {
        $path = "HKCU:\Software\Classes\$where\shell\kel"
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

# Your PATH, as Windows keeps it: in the registry, its entries as written
# (`%USERPROFILE%\...` stays that, not the folder it means today), and of the
# same kind. [Environment]::SetEnvironmentVariable would write those out.
function Set-UserPath([scriptblock]$change) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    $old = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
    $kind = if ($old) { $key.GetValueKind('Path') } else { 'ExpandString' }
    $entries = @($old -split ';' | Where-Object { $_ })
    $new = @(& $change $entries)
    $changed = ($new -join ';') -ne ($entries -join ';')
    if ($changed) {
        $key.SetValue('Path', ($new -join ';'), $kind)
        # Tells the programs that are open (Explorer, mostly) that the
        # environment changed, so what they start next sees the new PATH.
        # Setting a variable this way sends that message; it's gone again.
        [Environment]::SetEnvironmentVariable('KEL_PATH_CHANGED', '1', 'User')
        [Environment]::SetEnvironmentVariable('KEL_PATH_CHANGED', $null, 'User')
    }
    $key.Close()
    $changed
}

if ($Uninstall) {
    # A running exe can't be deleted (this Kel may be it), but it can be moved:
    # to the temp folder, which Windows empties now and then.
    if (Test-Path $exe) {
        $away = Join-Path $env:TEMP ('kel-{0:yyyyMMdd-HHmmss}.old' -f (Get-Date))
        Move-Item $exe $away -ErrorAction Stop
    }
    Remove-Item $dir -Recurse -ErrorAction SilentlyContinue
    Remove-Item $shortcut -ErrorAction SilentlyContinue
    Set-UserPath { param($entries) $entries | Where-Object { $_ -ne $dir } } | Out-Null
    Set-ExplorerMenu $exe -Remove
    Write-Host "[ok] Kel is uninstalled: its folder, the Start menu, PATH and Explorer's menu."
    Write-Host "     Your config and projects stay: $env:APPDATA\kel"
    exit 0
}

if (-not $From -or -not (Test-Path $From)) {
    Write-Host "Which kel.exe to install? kel.ps1 -From <path to kel.exe>"
    exit 1
}
$from = (Resolve-Path $From).Path

# Windows doesn't let a running exe be written over (an open Kel runs it), but it
# lets it be renamed. So the old one steps aside, then goes if no Kel runs it; one
# that still runs goes at a later install.
New-Item -ItemType Directory -Force $dir | Out-Null
if ($from -ne $exe) {
    if (Test-Path $exe) {
        Rename-Item $exe ('kel-{0:yyyyMMdd-HHmmss}.old' -f (Get-Date)) -ErrorAction Stop
    }
    Copy-Item $from $exe -ErrorAction Stop
}
Get-ChildItem $dir -Filter 'kel-*.old' | Remove-Item -ErrorAction SilentlyContinue
Write-Host "[ok] Kel:        $exe"

$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
$link.TargetPath = $exe
$link.Description = 'Kel, a terminal with tabs and projects'
$link.Save()
Write-Host "[ok] Start menu: Kel"

$added = Set-UserPath {
    param($entries)
    if ($entries -contains $dir) { $entries } else { @($entries) + $dir }
}
if ($added) {
    Write-Host "[ok] PATH:       added $dir. 'kel' works in shells opened from now on"
} else {
    Write-Host "[ok] PATH:       $dir is there"
}

if ($NoMenu) {
    Set-ExplorerMenu $exe -Remove
    Write-Host "[ok] Explorer:   no 'Open in Kel' in its menu"
} else {
    Set-ExplorerMenu $exe
    Write-Host "[ok] Explorer:   'Open in Kel' on a folder (Windows 11: Show more options)"
}

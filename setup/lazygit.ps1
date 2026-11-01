# Installs lazygit, git in the terminal: https://github.com/jesseduffield/lazygit
#
#   .\setup\lazygit.ps1           install it, if it isn't there yet
#   .\setup\lazygit.ps1 -DryRun   only say what it would do
#
# Safe to run again: whatever is already there is skipped. Afterwards open a new tab:
# tabs that were open before don't see newly installed programs (each process has
# its own copy of PATH, taken when it started).

param([switch]$DryRun)

# Reads PATH again from where Windows keeps it (the registry), so this script sees
# what it just installed. Programs that are already running keep their old copy.
function Update-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = "$machine;$user"
}

# Installs a tool with winget, unless `$Command` (the program you type) already runs.
# How it was installed doesn't matter: winget, choco, cargo, by hand.
function Install-Tool($Name, $Command, $WingetId) {
    $found = Get-Command $Command -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "[ok]      $Name is there: $($found.Source)"
        return
    }
    $install = "winget install --id $WingetId --exact --source winget --accept-package-agreements --accept-source-agreements"
    if ($DryRun) {
        Write-Host "[missing] $Name, would run: $install"
        return
    }
    Write-Host "[install] $Name`: $install"
    winget install --id $WingetId --exact --source winget --accept-package-agreements --accept-source-agreements
    Update-Path
    $found = Get-Command $Command -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "[ok]      $Name installed: $($found.Source)"
    } else {
        Write-Host "[failed]  $Name`: winget finished, but '$Command' isn't found (exit code $LASTEXITCODE)" -ForegroundColor Red
    }
}

Install-Tool 'lazygit' 'lazygit' 'JesseDuffield.lazygit'

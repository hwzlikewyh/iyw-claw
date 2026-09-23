param([ValidateSet('identity', 'check', 'elevate')][string]$Action)

$ErrorActionPreference = 'Stop'
$resultPath = Join-Path $PSScriptRoot 'iyw-permissions.ini'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
$elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$result = [ordered]@{ Sid = $identity.User.Value; Elevated = [int]$elevated; Launched = 0; Error = '' }
$exitCode = 0

function Assert-OriginalIdentity {
    $expected = $env:IYW_INSTALL_EXPECTED_SID
    if ($expected -and $expected -ne $identity.User.Value) {
        throw 'Elevation switched Windows accounts. Run installation from the original account; its environment has not been changed.'
    }
}

function Assert-PlainPath([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Installation path contains a symbolic link or junction: $cursor"
            }
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}

function Test-DirectoryAccess([string]$Path) {
    Assert-PlainPath $Path
    $probe = Join-Path $Path ('.iyw-install-probe-' + [guid]::NewGuid().ToString('N'))
    $renamed = $probe + '.renamed'
    try {
        [IO.Directory]::CreateDirectory($Path) | Out-Null
        $stream = [IO.File]::Open($probe, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $stream.WriteByte(0); $stream.Flush($true) } finally { $stream.Dispose() }
        [IO.File]::Move($probe, $renamed)
        [IO.File]::Delete($renamed)
    } catch {
        throw [IO.IOException]::new("Directory create/write/rename/delete check failed: $Path", $_.Exception)
    } finally {
        foreach ($temporary in @($probe, $renamed)) {
            if ([IO.File]::Exists($temporary)) {
                try { [IO.File]::Delete($temporary) } catch { Write-Warning "Probe cleanup failed: $temporary; $($_.Exception.Message)" }
            }
        }
    }
}

function Assert-InstallSpace([string]$Path) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class IywInstallDisk {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetDiskFreeSpaceExW(string path, out ulong available, out ulong total, out ulong free);
}
'@
    [ulong]$available = 0
    [ulong]$total = 0
    [ulong]$free = 0
    if (-not [IywInstallDisk]::GetDiskFreeSpaceExW($Path, [ref]$available, [ref]$total, [ref]$free)) {
        throw [ComponentModel.Win32Exception]::new([Runtime.InteropServices.Marshal]::GetLastWin32Error())
    }
    $required = [ulong]$env:IYW_INSTALL_REQUIRED_KB * 1024
    if ($available -lt $required) {
        throw "Insufficient installation space: $Path; required=$required bytes; available=$available bytes (the old application backup is retained)."
    }
}

function Test-InstallAccess {
    $root = [IO.Path]::GetFullPath($env:IYW_INSTALL_ROOT)
    $profileRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
    $environmentRoot = Join-Path $profileRoot '.iyw-claw'
    $directories = @($root, (Join-Path $root 'app'), (Join-Path $root 'staging'), (Join-Path $root 'logs'))
    foreach ($relative in @('runtime', 'runtime\cache\downloads', 'inventory', 'staging\environment', 'quarantine\environment', 'logs\environment')) {
        $directories += Join-Path $environmentRoot $relative
    }
    foreach ($directory in $directories) { Test-DirectoryAccess $directory }
    Assert-InstallSpace $root
    Write-Output 'Installation directory access and application disk-space checks passed.'
}

function Start-ElevatedInstaller {
    if ($elevated -or $env:IYW_INSTALL_EXPECTED_SID) { throw 'An elevated installation has already been attempted.' }
    $root = [IO.Path]::GetFullPath($env:IYW_INSTALL_ROOT)
    $arguments = '/IYW_ORIGINAL_SID=' + $identity.User.Value + ' /IYW_SELECTED_ROOT="' + $root + '" ' + $env:IYW_INSTALL_ARGUMENTS
    # Pass arguments directly to ShellExecute; do not evaluate them as PowerShell.
    $child = Start-Process -FilePath $env:IYW_INSTALL_EXECUTABLE -Verb RunAs -ArgumentList $arguments -WorkingDirectory $env:TEMP -WindowStyle Normal -PassThru
    $result.Launched = 1
    # Wait only for the installer, not the application it may start afterwards.
    $child.WaitForExit()
    $child.Refresh()
    if ($null -eq $child.ExitCode) { throw 'The elevated installer did not return an exit code.' }
    return $child.ExitCode
}

try {
    Assert-OriginalIdentity
    switch ($Action) {
        'check' { Test-InstallAccess }
        'elevate' { $exitCode = Start-ElevatedInstaller }
    }
} catch {
    $messages = @()
    $errorCursor = $_.Exception
    $exitCode = 1
    while ($null -ne $errorCursor) {
        $messages += $errorCursor.Message
        if ($errorCursor -is [UnauthorizedAccessException]) { $exitCode = 31 }
        if ($errorCursor -is [ComponentModel.Win32Exception] -and $errorCursor.NativeErrorCode -eq 5) { $exitCode = 31 }
        if ($errorCursor -is [ComponentModel.Win32Exception] -and $errorCursor.NativeErrorCode -eq 1223) { $exitCode = 1223 }
        $errorCursor = $errorCursor.InnerException
    }
    $result.Error = ($messages | Select-Object -Unique) -join ': '
    Write-Output $result.Error
} finally {
    $lines = @('[result]')
    foreach ($key in $result.Keys) { $lines += "$key=$($result[$key])".Replace("`r", ' ').Replace("`n", ' ') }
    [IO.File]::WriteAllLines($resultPath, $lines, [Text.Encoding]::Unicode)
}
exit $exitCode

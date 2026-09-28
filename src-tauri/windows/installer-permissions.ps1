param([ValidateSet('identity', 'check')][string]$Action)

$ErrorActionPreference = 'Stop'
$resultPath = Join-Path $PSScriptRoot 'iyw-permissions.ini'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
$elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$result = [ordered]@{ Sid = $identity.User.Value; Elevated = [int]$elevated; Root = ''; Error = '' }
$exitCode = 0
$fileRetryTimeout = [TimeSpan]::FromSeconds(15)
$fileRetryInitialDelay = 100
$fileRetryMaxDelay = 2000

function Assert-OriginalIdentity {
    $expected = $env:IYW_INSTALL_EXPECTED_SID
    if ($expected -and $expected -ne $identity.User.Value) {
        throw 'Elevation switched Windows accounts. Run installation from the original account; its environment has not been changed.'
    }
}

function Resolve-InstallRoot {
    $path = $env:IYW_INSTALL_ROOT
    if ([string]::IsNullOrWhiteSpace($path)) { throw 'Installation directory is empty. Please select an absolute directory.' }
    # Windows PowerShell 5.1 reads this script as ANSI; keep source text ASCII.
    # Unwrap legacy quoted paths, but reject quotes inside the actual path.
    if ($path.Length -ge 2 -and $path[0] -eq '"' -and $path[$path.Length - 1] -eq '"') {
        $path = $path.Substring(1, $path.Length - 2)
    }
    if ($path.IndexOfAny([IO.Path]::GetInvalidPathChars()) -ge 0 -or $path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)') {
        throw 'Installation directory is invalid. Select an absolute path without quotes or control characters.'
    }
    try { return [IO.Path]::GetFullPath($path) }
    catch { throw [IO.IOException]::new('Cannot resolve installation directory. Please select the directory again.', $_.Exception) }
}

function Assert-PlainPath([string]$Path) {
    try { $cursor = [IO.Path]::GetFullPath($Path) }
    catch { throw [IO.IOException]::new("Cannot resolve directory for access check: $Path", $_.Exception) }
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
    $started = [Diagnostics.Stopwatch]::StartNew()
    $delay = $fileRetryInitialDelay
    $attempt = 0
    while ($true) {
        $attempt++
        try {
            Invoke-DirectoryAccessProbe $Path
            if ($attempt -gt 1) { Write-Output "Directory access recovered: $Path; attempt=$attempt" }
            return
        } catch {
            $cause = $_.Exception.GetBaseException()
            $code = $cause.HResult -band 0xFFFF
            $remaining = $fileRetryTimeout.TotalMilliseconds - $started.Elapsed.TotalMilliseconds
            if ($code -notin @(5, 32, 33) -or $remaining -le 0) {
                throw [IO.IOException]::new("Directory create/write/rename/delete check failed: $Path; attempt=$attempt", $_.Exception)
            }
            Write-Output "Directory access retry: $Path; attempt=$attempt; win32_error=$code"
            Start-Sleep -Milliseconds ([int][Math]::Min($delay, $remaining))
            $delay = [Math]::Min($delay * 2, $fileRetryMaxDelay)
        }
    }
}

function Invoke-DirectoryAccessProbe([string]$Path) {
    $probe = Join-Path $Path ('.iyw-install-probe-' + [guid]::NewGuid().ToString('N'))
    $renamed = $probe + '.renamed'
    try {
        [IO.Directory]::CreateDirectory($Path) | Out-Null
        $stream = [IO.File]::Open($probe, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $stream.WriteByte(0); $stream.Flush($true) } finally { $stream.Dispose() }
        [IO.File]::Move($probe, $renamed)
        [IO.File]::Delete($renamed)
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
    [System.UInt64]$available = 0
    [System.UInt64]$total = 0
    [System.UInt64]$free = 0
    if (-not [IywInstallDisk]::GetDiskFreeSpaceExW($Path, [ref]$available, [ref]$total, [ref]$free)) {
        throw [ComponentModel.Win32Exception]::new([Runtime.InteropServices.Marshal]::GetLastWin32Error())
    }
    $required = [System.UInt64]$env:IYW_INSTALL_REQUIRED_KB * 1024
    if ($available -lt $required) {
        throw "Insufficient installation space: $Path; required=$required bytes; available=$available bytes (the old application backup is retained)."
    }
}

function Test-InstallAccess {
    $root = Resolve-InstallRoot
    $result.Root = $root
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

try {
    Assert-OriginalIdentity
    switch ($Action) {
        'check' { Test-InstallAccess }
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

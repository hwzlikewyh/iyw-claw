param(
    [string]$Directory, [string]$AppVersion, [int]$InstallerPid,
    [long]$OriginalBar, [long]$ProgressBar, [long]$StatusText
)
$ErrorActionPreference = 'Stop'
$arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "' + $Directory + '\installer-worker.ps1" -Directory "' + $Directory + '" -AppVersion ' + $AppVersion + ' -InstallerPid ' + $InstallerPid + ' -OriginalBar ' + $OriginalBar + ' -ProgressBar ' + $ProgressBar + ' -StatusText ' + $StatusText
# NSIS stores a native System.dll in Directory; Add-Type must not resolve it as a .NET reference.
$worker = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -ArgumentList $arguments -WorkingDirectory $PSHOME -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $Directory 'worker.stdout.log') -RedirectStandardError (Join-Path $Directory 'worker.stderr.log')
Write-Output $worker.Id

param(
    [Parameter(Mandatory = $true)][string]$Directory,
    [Parameter(Mandatory = $true)][string]$AppVersion,
    [Parameter(Mandatory = $true)][int]$InstallerPid,
    [long]$OriginalBar = 0, [long]$ProgressBar = 0, [long]$StatusText = 0,
    [long]$StageText = 0
)
$ErrorActionPreference = 'Stop'
$phaseTimeout = [TimeSpan]::FromMinutes(20)
$pollMilliseconds = 250
$statePath = Join-Path $Directory 'environment.status.ini'
$progressPath = Join-Path $Directory 'environment.progress.ini'
$logPath = Join-Path $Directory 'environment-worker.log'
$commitRequest = Join-Path $Directory 'environment.commit'
$cancelRequest = Join-Path $Directory 'environment.cancel'
$parent = $null
$child = $null
$script:displayed = 0
$script:environmentProgress = 0
$script:progressWarning = $false

function Write-State([string]$State, [int]$Code) {
    $text = "[result]`r`nState=$State`r`nCode=$Code`r`n"
    # A partial snapshot read is not a completion signal.
    [IO.File]::WriteAllText($statePath, $text, [Text.Encoding]::Unicode)
}

function Assert-Running {
    if ($parent.HasExited -or [IO.File]::Exists($cancelRequest)) {
        throw [OperationCanceledException]::new('Installer cancelled or exited.')
    }
}

function Update-Progress {
    $value = 0
    if ([int]::TryParse([IywInstallerNative]::Read($progressPath, 'Value'), [ref]$value)) {
        $script:environmentProgress = [Math]::Max($script:environmentProgress, [Math]::Min($value, 10000))
    }
    $position = [IywInstallerNative]::Message($OriginalBar, 0x408, 0)
    $maximum = [IywInstallerNative]::Message($OriginalBar, 0x407, 0)
    $appProgress = if ($maximum -gt 0) { [Math]::Min(1, $position / $maximum) } else { 0 }
    if ([IO.File]::Exists($commitRequest)) { $appProgress = 1 }
    $combined = [int](3500 * $appProgress + 0.6 * $script:environmentProgress)
    $script:displayed = [Math]::Max($script:displayed, [Math]::Min(9500, $combined))
    $phase = [IywInstallerNative]::Read($progressPath, 'Phase')
    $stage = if ($phase -in @('commit', 'committed')) { 'complete' }
        elseif ($script:environmentProgress -gt 0 -or $appProgress -gt 0) { 'initialize' }
        else { 'prepare' }
    $updated = [IywInstallerNative]::Stage($StageText, $stage)
    $updated = [IywInstallerNative]::Show($ProgressBar, $StatusText, $script:displayed) -and $updated
    if (-not $updated -and -not $script:progressWarning) {
        $warning = 'Progress control update timed out or failed; retrying on the next poll.'
        if ($null -ne $child) { $child.WriteDiagnostic($warning) }
        else { [IO.File]::AppendAllText($logPath, $warning + "`r`n") }
        $script:progressWarning = $true
    }
}

function Invoke-Phase([string]$Phase, [string]$Transaction = '') {
    $executable = Join-Path $Directory 'iyw-environment.exe'
    $arguments = 'install --phase ' + $Phase + ' --app-version ' + $AppVersion + ' --progress-file "' + $progressPath + '"'
    if ($Transaction) { $arguments += ' --transaction-id ' + $Transaction }
    $script:child = [IywInstallerChild]::new($executable, $arguments, $logPath)
    try {
        $started = [Diagnostics.Stopwatch]::StartNew()
        while (-not $child.HasExited) {
            if ($Phase -eq 'prepare') { Assert-Running }
            if ($started.Elapsed -gt $phaseTimeout) { throw [TimeoutException]::new('Initialization timed out.') }
            Update-Progress
            Start-Sleep -Milliseconds $pollMilliseconds
        }
        $code = $child.Finish()
        Update-Progress
        return $code
    } finally { $child.Dispose(); $script:child = $null }
}

try {
    Write-State 'starting' 0
    if ($AppVersion -notmatch '^\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$') { throw 'Invalid application version.' }
    $parent = Get-Process -Id $InstallerPid
    $null = $parent.Handle
    Assert-Running
    Add-Type -Path (Join-Path $Directory 'installer-worker-native.cs')
    foreach ($window in @($OriginalBar, $ProgressBar, $StatusText, $StageText)) {
        if ($window -eq 0) { continue }
        [uint32]$owner = 0
        [void][IywInstallerNative]::GetWindowThreadProcessId([IntPtr]$window, [ref]$owner)
        if ($owner -ne $InstallerPid) { throw 'Progress control does not belong to installer.' }
    }
    Write-State 'preparing' 0
    $code = Invoke-Phase 'prepare'
    if ($code -ne 0) { Write-State 'failed' $code; exit $code }
    $transaction = [IywInstallerNative]::Read($progressPath, 'Transaction')
    if ($transaction -notmatch '^[a-f0-9]{32}$') { throw 'Prepared transaction identity missing.' }
    Write-State 'prepared' 0
    $waiting = [Diagnostics.Stopwatch]::StartNew()
    while (-not [IO.File]::Exists($commitRequest)) {
        Assert-Running
        if ($waiting.Elapsed -gt $phaseTimeout) { throw [TimeoutException]::new('Application preparation timed out.') }
        Update-Progress
        Start-Sleep -Milliseconds $pollMilliseconds
    }
    Assert-Running
    Write-State 'committing' 0
    $code = Invoke-Phase 'commit' $transaction
    if ($code -ne 0) { Write-State 'failed' $code; exit $code }
    Write-State 'committed' 0
} catch [OperationCanceledException] {
    Write-State 'cancelled' 1
} catch {
    [IO.File]::AppendAllText($logPath, $_.Exception.ToString() + [Environment]::NewLine)
    Write-State 'failed' 1
} finally {
    if ($null -ne $child) { $child.Dispose() }
    if ($null -ne $parent) { $parent.Dispose() }
}

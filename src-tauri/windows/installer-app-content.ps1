param([Parameter(Mandatory = $true)][string]$BackupDirectory)

$ErrorActionPreference = 'Stop'
try {
    if (-not (Test-Path -LiteralPath $BackupDirectory)) { exit 0 }
    $root = Get-Item -LiteralPath $BackupDirectory -Force
    if (-not $root.PSIsContainer -or ($root.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Application backup is not a regular directory.'
    }
    # Only known package entries may be removed; preserve unknown content.
    $files = @('iyw-claw.exe', 'iyw-environment.exe', 'uninstall.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md')
    $directories = @('resources', 'environment', 'xinghe-resources')
    foreach ($entry in Get-ChildItem -LiteralPath $BackupDirectory -Force) {
        $known = if ($entry.PSIsContainer) { $directories -contains $entry.Name } else { $files -contains $entry.Name }
        if (-not $known -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            Write-Output 'Application backup contains non-package content; retaining the whole backup.'
            exit 1
        }
    }
    exit 0
} catch {
    Write-Output "Cannot inspect application backup; cleanup stopped: $($_.Exception.Message)"
    exit 2
}

#
# iyw-claw Server installer for Windows
# Usage:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/hwzlikewyh/iyw-claw/v0.1.93/install.ps1))) -Version v0.1.93
#   .\install.ps1 -Version v0.1.93
#

param(
    [string]$Version = "",
    [string]$InstallDir = "$env:LOCALAPPDATA\iyw-claw",
    [switch]$NoCleanup
)

$ErrorActionPreference = "Stop"
$Repo = "hwzlikewyh/iyw-claw"
$Artifact = "iyw-claw-server-windows-x64"
$MinHttpOnlyVersion = [version]'0.1.93'
$MinisignPublicKey = 'RWQs3MShTUgMUqJIgj5NzBI/EZyDJcjPnIGgzNUuBvd21qtV152OjF9X'
$StableTagPattern = '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
$MaxArchiveBytes = [long](512MB)
$MaxExpandedBytes = [long](1GB)
$MaxZipEntries = 50000

if (-not [Environment]::Is64BitOperatingSystem) {
    Write-Error 'The HTTP-only server installer currently supports Windows x64 only.'
    exit 1
}

# The server owns the built-in MCP endpoint over Streamable HTTP, so it is the
# only executable managed by this installer.
$ManagedBins = @("iyw-claw-server")

# Legacy built-in MCP companions are not managed binaries anymore. Remove only
# the exact old filenames from the selected install directory.
$LegacyMcpVersionPattern = '^iyw-claw-mcp-(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$'

# Stale iyw-claw-server binaries elsewhere in PATH are removed by default so
# the user's command always runs the freshly installed binary. Pass -NoCleanup
# (or set IYW_CLAW_NO_CLEANUP=1) to disable.
$Cleanup = -not $NoCleanup
if ($env:IYW_CLAW_NO_CLEANUP -eq "1") {
    $Cleanup = $false
}

function Get-SafeCanonicalPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) { throw 'Path is empty' }
    $full = [IO.Path]::GetFullPath($Path)
    $missing = @()
    $cursor = $full
    while (-not (Test-Path -LiteralPath $cursor -ErrorAction Stop)) {
        $leaf = [IO.Path]::GetFileName($cursor)
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ([string]::IsNullOrWhiteSpace($leaf) -or [string]::IsNullOrWhiteSpace($parent)) {
            throw "Cannot resolve an existing ancestor for path: $Path"
        }
        $missing = @($leaf) + $missing
        $cursor = $parent
    }
    if ($missing.Count -gt 0) {
        $existing = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
        if (-not $existing.PSIsContainer) {
            throw "The nearest existing path is not a directory: $cursor"
        }
    }
    $probe = $cursor
    while (-not [string]::IsNullOrWhiteSpace($probe)) {
        $item = Get-Item -LiteralPath $probe -Force -ErrorAction Stop
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Path contains a symbolic link or junction: $probe"
        }
        $parent = [IO.Path]::GetDirectoryName($probe)
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $probe) { break }
        $probe = $parent
    }
    $canonical = (Get-Item -LiteralPath $cursor -Force -ErrorAction Stop).FullName
    foreach ($part in $missing) { $canonical = Join-Path $canonical $part }
    return [IO.Path]::GetFullPath($canonical)
}

function Get-CanonicalPath([string]$Path) {
    if (-not $Path) { return "" }
    try { return Get-SafeCanonicalPath $Path } catch { return [IO.Path]::GetFullPath($Path) }
}

function Test-NonEmptyFile([string]$Path) {
    if (-not $Path) { return $false }
    try {
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        return (-not $item.PSIsContainer) `
            -and (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) `
            -and $item.Length -gt 0
    } catch {
        return $false
    }
}

function Test-LegacyMcpName([string]$Name) {
    if (-not $Name) { return $false }
    $stem = $Name
    if ($stem.EndsWith('.exe', [StringComparison]::OrdinalIgnoreCase)) {
        $stem = $stem.Substring(0, $stem.Length - 4)
    }
    return $stem -eq 'iyw-claw-mcp' -or $stem -match $LegacyMcpVersionPattern
}

function Get-ValidatedLegacyMcpPath([string]$Directory, [object]$Item) {
    if (-not (Test-LegacyMcpName $Item.Name)) { throw "Not a legacy MCP candidate: $($Item.Name)" }
    $current = Get-Item -LiteralPath $Item.FullName -Force -ErrorAction Stop
    if (-not ($current -is [IO.FileInfo]) -or $current.PSIsContainer) {
        throw "Legacy MCP candidate is not a regular file: $($current.FullName)"
    }
    if (($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Legacy MCP candidate is a reparse point: $($current.FullName)"
    }
    $expectedParent = Get-SafeCanonicalPath $Directory
    $actualParent = Get-SafeCanonicalPath $current.DirectoryName
    if (-not [string]::Equals($actualParent, $expectedParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Legacy MCP candidate escaped the install directory: $($current.FullName)"
    }
    return $current.FullName
}

function Get-LegacyMcpFiles([string]$Directory) {
    if (-not (Test-Path -LiteralPath $Directory -ErrorAction Stop)) { return @() }
    if (-not (Test-Path -LiteralPath $Directory -PathType Container -ErrorAction Stop)) {
        throw "Cannot inspect legacy MCP files because the install path is not a directory: $Directory"
    }
    try {
        $matches = @()
        foreach ($item in @(Get-ChildItem -LiteralPath $Directory -Force -ErrorAction Stop)) {
            if (-not (Test-LegacyMcpName $item.Name)) { continue }
            $matches += Get-ValidatedLegacyMcpPath $Directory $item
        }
        return $matches
    } catch {
        throw "Cannot inspect legacy MCP files in ${Directory}: $($_.Exception.Message)"
    }
}

function Test-ScopedProcessName([string]$Name, [string]$Kind) {
    if ($Kind -eq 'server') {
        return [string]::Equals($Name, 'iyw-claw-server.exe', [StringComparison]::OrdinalIgnoreCase)
    }
    if ($Kind -eq 'legacy-mcp') { return Test-LegacyMcpName $Name }
    throw "Unknown scoped process kind: $Kind"
}

function Get-CurrentUserSid {
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    if ([string]::IsNullOrWhiteSpace($sid)) { throw 'Current user SID is unavailable' }
    return $sid
}

function New-ScopedProcessIdentity([object]$CimProcess, [string]$Kind) {
    if (-not (Test-ScopedProcessName $CimProcess.Name $Kind)) {
        throw "Unexpected $Kind process name: $($CimProcess.Name)"
    }
    if ([string]::IsNullOrWhiteSpace($CimProcess.ExecutablePath)) {
        throw 'executable path is unavailable'
    }
    $owner = Invoke-CimMethod -InputObject $CimProcess -MethodName GetOwnerSid -ErrorAction Stop
    if ($owner.ReturnValue -ne 0 -or [string]::IsNullOrWhiteSpace($owner.Sid)) {
        throw 'owner SID is unavailable'
    }
    $processId = [int]$CimProcess.ProcessId
    $runtime = Get-Process -Id $processId -ErrorAction Stop
    $startTicks = $runtime.StartTime.ToUniversalTime().Ticks
    if ([string]::IsNullOrWhiteSpace($runtime.Path)) { throw 'runtime executable path is unavailable' }
    $cimExecutable = Get-SafeCanonicalPath $CimProcess.ExecutablePath
    $runtimeExecutable = Get-SafeCanonicalPath $runtime.Path
    if (-not [string]::Equals($cimExecutable, $runtimeExecutable, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'CIM and runtime executable identities differ'
    }
    $parentPath = Split-Path -Parent $cimExecutable
    if ([string]::IsNullOrWhiteSpace($parentPath)) { throw 'executable parent is unavailable' }
    $parent = Get-SafeCanonicalPath $parentPath
    $leaf = Split-Path -Leaf $cimExecutable
    if (-not [string]::Equals($leaf, $CimProcess.Name, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'executable name does not match the process name'
    }
    return [PSCustomObject]@{
        ProcessId = $processId; StartTimeUtcTicks = $startTicks; OwnerSid = $owner.Sid
        ExecutablePath = $cimExecutable; ParentPath = $parent; Name = $CimProcess.Name
        RuntimeProcess = $runtime
    }
}

function Get-ScopedProcessSnapshots([string]$Directory, [string]$Kind) {
    if (-not (Test-Path -LiteralPath $Directory -ErrorAction Stop)) { return @() }
    if (-not (Test-Path -LiteralPath $Directory -PathType Container -ErrorAction Stop)) {
        throw "Process scope is not a directory: $Directory"
    }
    $target = Get-SafeCanonicalPath $Directory
    $currentSid = Get-CurrentUserSid
    $matched = @()
    foreach ($process in @(Get-CimInstance Win32_Process -ErrorAction Stop)) {
        if (-not (Test-ScopedProcessName $process.Name $Kind)) { continue }
        try {
            $identity = New-ScopedProcessIdentity $process $Kind
        } catch {
            throw "Cannot inspect $Kind process $($process.ProcessId): $($_.Exception.Message)"
        }
        $sameOwner = [string]::Equals($identity.OwnerSid, $currentSid, [StringComparison]::OrdinalIgnoreCase)
        $sameParent = [string]::Equals($identity.ParentPath, $target, [StringComparison]::OrdinalIgnoreCase)
        if ($sameOwner -and $sameParent) { $matched += $identity }
    }
    return $matched
}

function Confirm-ScopedProcessIdentity([object]$Expected, [string]$Directory, [string]$Kind) {
    $processId = [int]$Expected.ProcessId
    $records = @(Get-CimInstance Win32_Process -Filter "ProcessId = $processId" -ErrorAction Stop)
    if ($records.Count -eq 0) { return $null }
    if ($records.Count -ne 1) { throw "PID $processId returned multiple process records" }
    $actual = New-ScopedProcessIdentity $records[0] $Kind
    $identityChanged = $actual.ProcessId -ne $Expected.ProcessId `
        -or $actual.StartTimeUtcTicks -ne $Expected.StartTimeUtcTicks `
        -or -not [string]::Equals($actual.OwnerSid, $Expected.OwnerSid, [StringComparison]::Ordinal) `
        -or -not [string]::Equals($actual.ExecutablePath, $Expected.ExecutablePath, [StringComparison]::OrdinalIgnoreCase)
    if ($identityChanged) { throw "Process identity changed before stop for PID $processId" }
    $target = Get-SafeCanonicalPath $Directory
    $currentSid = Get-CurrentUserSid
    if (-not [string]::Equals($actual.OwnerSid, $currentSid, [StringComparison]::OrdinalIgnoreCase) `
        -or -not [string]::Equals($actual.ParentPath, $target, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Process scope changed before stop for PID $processId"
    }
    return $actual.RuntimeProcess
}

function Stop-OneScopedProcess([object]$Snapshot, [string]$Directory, [string]$Kind) {
    try {
        $liveProcess = Confirm-ScopedProcessIdentity $Snapshot $Directory $Kind
        if ($null -ne $liveProcess) {
            Stop-Process -InputObject $liveProcess -Force -ErrorAction Stop
        }
        return $true
    } catch {
        Write-Host "  failed to stop $Kind process $($Snapshot.ProcessId) - $($_.Exception.Message)"
        return $false
    }
}

function Stop-ScopedProcesses([string]$Directory, [string]$Kind, [int]$Attempts) {
    for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
        $snapshots = @(Get-ScopedProcessSnapshots $Directory $Kind)
        if ($snapshots.Count -eq 0) { return $true }
        foreach ($snapshot in $snapshots) {
            if (-not (Stop-OneScopedProcess $snapshot $Directory $Kind)) { return $false }
        }
        Start-Sleep -Milliseconds 500
    }
    if (@(Get-ScopedProcessSnapshots $Directory $Kind).Count -gt 0) {
        Write-Host "$Kind process(es) are still running."
        return $false
    }
    return $true
}

function Get-LegacyMcpProcesses([string]$Directory) {
    return @(Get-ScopedProcessSnapshots $Directory 'legacy-mcp')
}

function Stop-LegacyMcpProcesses([string]$Directory) {
    return (Stop-ScopedProcesses $Directory 'legacy-mcp' 3)
}

function Assert-ZipPathSegment([string]$Segment, [string]$EntryName) {
    if ([string]::IsNullOrWhiteSpace($Segment) -or $Segment -eq '.' -or $Segment -eq '..') {
        throw "Unsafe ZIP entry path: $EntryName"
    }
    if ($Segment.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0 `
        -or $Segment.EndsWith(' ') -or $Segment.EndsWith('.')) {
        throw "ZIP entry has an invalid Windows path segment: $EntryName"
    }
    if ($Segment -match '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?$') {
        throw "ZIP entry uses a reserved Windows path segment: $EntryName"
    }
    if ($Segment -like 'iyw-claw-mcp*') {
        throw "ZIP entry contains forbidden legacy MCP content: $EntryName"
    }
}

function ConvertTo-SafeZipPath([string]$EntryName) {
    if ([string]::IsNullOrWhiteSpace($EntryName) -or $EntryName.Contains('\') `
        -or $EntryName.Contains(':') -or $EntryName.StartsWith('/')) {
        throw "Unsafe ZIP entry path: $EntryName"
    }
    $isDirectory = $EntryName.EndsWith('/')
    $trimmed = $EntryName.TrimEnd('/')
    if ([string]::IsNullOrWhiteSpace($trimmed)) { throw "Unsafe ZIP entry path: $EntryName" }
    $parts = $trimmed.Split('/')
    foreach ($part in $parts) { Assert-ZipPathSegment $part $EntryName }
    return [PSCustomObject]@{ Path = ($parts -join '/'); IsDirectory = $isDirectory }
}

function Assert-ZipAllowedPath([object]$SafePath, [string]$EntryName) {
    $artifactPrefix = [regex]::Escape($Artifact)
    if ($SafePath.Path -eq $Artifact) {
        if (-not $SafePath.IsDirectory) { throw "ZIP artifact root is not a directory: $EntryName" }
        return
    }
    if ($SafePath.Path -eq "$Artifact/iyw-claw-server.exe") {
        if ($SafePath.IsDirectory) { throw "ZIP server executable is a directory: $EntryName" }
        return
    }
    if ($SafePath.Path -eq "$Artifact/web") {
        if (-not $SafePath.IsDirectory) { throw "ZIP web root is not a directory: $EntryName" }
        return
    }
    if ($SafePath.Path -notmatch "^$artifactPrefix/web/.+") {
        throw "Unexpected ZIP entry: $EntryName"
    }
}

function Get-ZipParentPaths([string]$Path) {
    $parts = $Path.Split('/')
    $parents = @()
    for ($index = 1; $index -lt $parts.Count; $index++) {
        $parents += ($parts[0..($index - 1)] -join '/')
    }
    return $parents
}

function Add-ZipPathToInventory([object]$SafePath, [string]$EntryName, [object]$State) {
    foreach ($parent in @(Get-ZipParentPaths $SafePath.Path)) {
        if ($State.FilePaths.Contains($parent)) { throw "ZIP path descends from a file: $EntryName" }
        $null = $State.ParentPaths.Add($parent)
    }
    if (-not $SafePath.IsDirectory -and $State.ParentPaths.Contains($SafePath.Path)) {
        throw "ZIP file conflicts with a child path: $EntryName"
    }
    if (-not $SafePath.IsDirectory) { $null = $State.FilePaths.Add($SafePath.Path) }
}

function Assert-ZipEntryType([object]$Entry, [object]$SafePath) {
    $attributes = [int64]$Entry.ExternalAttributes
    $dosAttributes = $attributes -band 0xFFFF
    $unixType = ($attributes -shr 16) -band 0xF000
    if (($dosAttributes -band [int][IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "ZIP entry is marked as a reparse point: $($Entry.FullName)"
    }
    if ($unixType -ne 0 -and $unixType -ne 0x4000 -and $unixType -ne 0x8000) {
        throw "ZIP entry is not a regular file or directory: $($Entry.FullName)"
    }
    if (($unixType -eq 0x4000 -and -not $SafePath.IsDirectory) `
        -or ($unixType -eq 0x8000 -and $SafePath.IsDirectory)) {
        throw "ZIP entry type conflicts with its path: $($Entry.FullName)"
    }
    if ($SafePath.IsDirectory -and $Entry.Length -ne 0) {
        throw "ZIP directory entry contains file data: $($Entry.FullName)"
    }
}

function Assert-ArchiveFile([string]$ZipPath) {
    $zipItem = Get-Item -LiteralPath $ZipPath -Force -ErrorAction Stop
    if (-not ($zipItem -is [IO.FileInfo]) -or $zipItem.PSIsContainer `
        -or ($zipItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Release archive is not a regular file: $ZipPath"
    }
    if ($zipItem.Length -le 0 -or $zipItem.Length -gt $MaxArchiveBytes) {
        throw "Release archive size $($zipItem.Length) is outside the allowed limit of $MaxArchiveBytes bytes"
    }
}

function Get-ZipInventory([string]$ZipPath) {
    Add-Type -AssemblyName System.IO.Compression
    Assert-ArchiveFile $ZipPath
    $stream = [IO.File]::Open($ZipPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Read, $false)
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $filePaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $parentPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $pathState = [PSCustomObject]@{ FilePaths = $filePaths; ParentPaths = $parentPaths }
    $entryCount = 0
    [long]$expandedBytes = 0
    try {
        foreach ($entry in $archive.Entries) {
            $entryCount++
            if ($entryCount -gt $MaxZipEntries) { throw "ZIP contains more than $MaxZipEntries entries" }
            $safePath = ConvertTo-SafeZipPath $entry.FullName
            if (-not $seen.Add($safePath.Path)) { throw "Duplicate normalized ZIP entry: $($entry.FullName)" }
            Add-ZipPathToInventory $safePath $entry.FullName $pathState
            if ($entry.Length -lt 0 -or $entry.Length -gt $MaxExpandedBytes `
                -or $expandedBytes -gt ($MaxExpandedBytes - $entry.Length)) {
                throw "ZIP expanded size exceeds $MaxExpandedBytes bytes"
            }
            $expandedBytes += $entry.Length
            Assert-ZipEntryType $entry $safePath
            Assert-ZipAllowedPath $safePath $entry.FullName
        }
    } finally {
        $archive.Dispose()
        $stream.Dispose()
    }
    if (-not $filePaths.Contains("$Artifact/iyw-claw-server.exe") `
        -or -not $filePaths.Contains("$Artifact/web/index.html")) {
        throw 'ZIP is missing the server executable or web/index.html'
    }
    $script:ExpectedExpandedBytes = $expandedBytes
    Write-Host "ZIP inventory verified: $entryCount entries, $expandedBytes expanded bytes."
}

function Assert-ExtractedBundle([string]$Root) {
    $rootItem = Get-Item -LiteralPath $Root -Force -ErrorAction Stop
    if (-not $rootItem.PSIsContainer -or
        ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Extracted bundle root is not a normal directory: $Root"
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $Root -Force -ErrorAction Stop)) {
        if ($item.Name -notin @('iyw-claw-server.exe', 'web')) {
            throw "Unexpected extracted bundle entry: $($item.FullName)"
        }
    }
    [long]$expandedBytes = 0
    $entryCount = 0
    foreach ($item in @(Get-ChildItem -LiteralPath $Root -Recurse -Force -ErrorAction Stop)) {
        $entryCount++
        if ($entryCount -gt $MaxZipEntries) { throw "Extracted bundle exceeds $MaxZipEntries entries" }
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Extracted bundle contains a reparse point: $($item.FullName)"
        }
        if ($item.Name -like 'iyw-claw-mcp*') {
            throw "Extracted bundle contains forbidden legacy MCP content: $($item.FullName)"
        }
        if (-not $item.PSIsContainer) {
            if ($item.Length -gt $MaxExpandedBytes - $expandedBytes) {
                throw "Extracted bundle exceeds $MaxExpandedBytes bytes"
            }
            $expandedBytes += $item.Length
        }
    }
    if (-not (Test-NonEmptyFile (Join-Path $Root 'iyw-claw-server.exe')) `
        -or -not (Test-NonEmptyFile (Join-Path $Root 'web' 'index.html'))) {
        throw 'Extracted bundle is missing iyw-claw-server.exe or web/index.html'
    }
    if ($expandedBytes -ne $script:ExpectedExpandedBytes) {
        throw "Extracted bundle size $expandedBytes differs from ZIP inventory $script:ExpectedExpandedBytes"
    }
}

function Assert-WritableDirectory([string]$Path) {
    $null = Get-SafeCanonicalPath $Path
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    $safe = Get-SafeCanonicalPath $Path
    $probe = Join-Path $safe ('.iyw-claw-write-' + [Guid]::NewGuid().ToString('N'))
    try {
        $stream = [IO.File]::Open($probe, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $stream.Dispose()
    } finally {
        Remove-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
    }
}

function Assert-NormalDirectoryTree([string]$Path, [string]$Label) {
    $root = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if (-not $root.PSIsContainer -or ($root.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label root is not a normal directory: $Path"
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $Path -Recurse -Force -ErrorAction Stop)) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Label contains a reparse point: $($item.FullName)"
        }
    }
}

function Assert-TargetLayout {
    $script:InstallDir = Get-SafeCanonicalPath $InstallDir
    $script:DestBin = Join-Path $InstallDir 'iyw-claw-server.exe'
    $script:WebDir = Join-Path $InstallDir 'web'
    if ($InstallDir -eq $WebDir -or $InstallDir.StartsWith("$WebDir\", [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Web directory must not equal or contain the binary installation directory'
    }
    if (Test-Path -LiteralPath $DestBin) {
        $item = Get-Item -LiteralPath $DestBin -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Binary target is not a regular file: $DestBin"
        }
    }
    if (Test-Path -LiteralPath $WebDir) {
        Assert-NormalDirectoryTree $WebDir 'Existing web tree'
    }
    $null = Get-SafeCanonicalPath $WebDir
}

function Prepare-TargetStaging([string]$BundleRoot) {
    Assert-TargetLayout
    Assert-WritableDirectory $InstallDir
    $script:ServerTxnDir = Join-Path $InstallDir ('.iyw-claw-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $ServerTxnDir | Out-Null
    $null = Get-SafeCanonicalPath $ServerTxnDir
    $newServer = Join-Path $ServerTxnDir 'new-server.exe'
    Copy-Item -LiteralPath (Join-Path $BundleRoot 'iyw-claw-server.exe') `
        -Destination $newServer
    $stagedVersion = Read-BinVersion $newServer
    if ($stagedVersion -ne $TargetVer) { throw "Staged server version is $stagedVersion; expected $TargetVer" }

    $webParent = Split-Path -Parent $WebDir
    Assert-WritableDirectory $webParent
    $script:WebTxnDir = Join-Path $webParent ('.iyw-claw-web-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $WebTxnDir | Out-Null
    $null = Get-SafeCanonicalPath $WebTxnDir
    $newWeb = Join-Path $WebTxnDir 'new-web'
    New-Item -ItemType Directory -Path $newWeb | Out-Null
    foreach ($item in @(Get-ChildItem -LiteralPath (Join-Path $BundleRoot 'web') -Force)) {
        Copy-Item -LiteralPath $item.FullName -Destination $newWeb -Recurse -Force
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $newWeb -Recurse -Force)) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Staged web assets contain a reparse point: $($item.FullName)"
        }
    }
    if (-not (Test-NonEmptyFile (Join-Path $newWeb 'index.html'))) { throw 'Staged web assets are incomplete' }
}

function Assert-SameVolume([string]$Source, [string]$Destination) {
    $sourceRoot = [IO.Path]::GetPathRoot((Get-SafeCanonicalPath $Source))
    $destinationRoot = [IO.Path]::GetPathRoot((Get-SafeCanonicalPath $Destination))
    if (-not [string]::Equals($sourceRoot, $destinationRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Transaction quarantine must stay on the source volume: $Source"
    }
}

function Move-LegacyMcpToQuarantine([string]$Directory) {
    $files = @(Get-LegacyMcpFiles $Directory)
    if ($files.Count -eq 0) { return }
    if ([string]::IsNullOrWhiteSpace($script:LegacyQuarantineDir)) {
        $script:LegacyQuarantineDir = Join-Path $ServerTxnDir 'legacy-mcp-quarantine'
        New-Item -ItemType Directory -Path $LegacyQuarantineDir -ErrorAction Stop | Out-Null
    }
    $null = Get-SafeCanonicalPath $LegacyQuarantineDir
    foreach ($path in $files) {
        $item = Get-Item -LiteralPath $path -Force -ErrorAction Stop
        $validated = Get-ValidatedLegacyMcpPath $Directory $item
        $destination = Join-Path $LegacyQuarantineDir $item.Name
        Assert-SameVolume $validated $destination
        if (Test-Path -LiteralPath $destination) {
            throw "Legacy MCP quarantine collision: $destination"
        }
        Move-Item -LiteralPath $validated -Destination $destination -ErrorAction Stop
        $script:LegacyQuarantine += [PSCustomObject]@{
            Original = $validated; Quarantined = $destination; Restored = $false
        }
        Write-Host "  quarantined legacy MCP file $validated"
    }
    if (@(Get-LegacyMcpFiles $Directory).Count -gt 0) {
        throw 'Legacy MCP files remained after quarantine'
    }
}

function Restore-LegacyMcpQuarantine([string]$Directory) {
    $errors = @()
    foreach ($entry in @($script:LegacyQuarantine)) {
        if ($entry.Restored) { continue }
        try {
            $backup = Get-Item -LiteralPath $entry.Quarantined -Force -ErrorAction Stop
            $quarantined = Get-ValidatedLegacyMcpPath $LegacyQuarantineDir $backup
            if (Test-Path -LiteralPath $entry.Original) {
                throw "Legacy MCP restore target already exists: $($entry.Original)"
            }
            Move-Item -LiteralPath $quarantined -Destination $entry.Original -ErrorAction Stop
            $entry.Restored = $true
            $item = Get-Item -LiteralPath $entry.Original -Force -ErrorAction Stop
            $null = Get-ValidatedLegacyMcpPath $Directory $item
        } catch {
            $errors += $_.Exception.Message
        }
    }
    if ($errors.Count -gt 0) { throw ($errors -join '; ') }
    foreach ($entry in @($script:LegacyQuarantine)) {
        if (-not $entry.Restored) {
            throw "Legacy MCP restore was not proven: $($entry.Original)"
        }
        $restored = Get-Item -LiteralPath $entry.Original -Force -ErrorAction Stop
        $null = Get-ValidatedLegacyMcpPath $Directory $restored
    }
    if ($LegacyQuarantineDir -and (Test-Path -LiteralPath $LegacyQuarantineDir) `
        -and @(Get-ChildItem -LiteralPath $LegacyQuarantineDir -Force -ErrorAction Stop).Count -gt 0) {
        throw "Legacy MCP quarantine still contains backup files: $LegacyQuarantineDir"
    }
}

function Assert-LegacyQuarantineIntegrity {
    if ($script:LegacyQuarantine.Count -eq 0) { return }
    if (-not (Test-Path -LiteralPath $LegacyQuarantineDir -PathType Container)) {
        throw "Legacy MCP quarantine is missing: $LegacyQuarantineDir"
    }
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in @($script:LegacyQuarantine)) { $null = $expected.Add($entry.Quarantined) }
    foreach ($item in @(Get-ChildItem -LiteralPath $LegacyQuarantineDir -Force -ErrorAction Stop)) {
        if (-not ($item -is [IO.FileInfo]) -or $item.PSIsContainer `
            -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Unexpected item in legacy MCP quarantine: $($item.FullName)"
        }
        if (-not $expected.Contains($item.FullName)) {
            throw "Untracked item in legacy MCP quarantine: $($item.FullName)"
        }
        $null = Get-ValidatedLegacyMcpPath $LegacyQuarantineDir $item
    }
}

function Remove-GeneratedDirectory([string]$Path, [string]$Parent, [string]$Label) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) { return }
    $safePath = Get-SafeCanonicalPath $Path
    $safeParent = Get-SafeCanonicalPath $Parent
    $actualParent = Get-SafeCanonicalPath (Split-Path -Parent $safePath)
    if (-not [string]::Equals($actualParent, $safeParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label directory escaped its expected parent: $safePath"
    }
    Assert-NormalDirectoryTree $safePath $Label
    Remove-Item -LiteralPath $safePath -Recurse -Force -ErrorAction Stop
    if (Test-Path -LiteralPath $safePath) { throw "$Label directory still exists: $safePath" }
}

function Remove-LegacyMcpQuarantine {
    if ($script:LegacyQuarantine.Count -eq 0) { return }
    Assert-LegacyQuarantineIntegrity
    try {
        Remove-GeneratedDirectory $LegacyQuarantineDir $ServerTxnDir 'Legacy MCP quarantine'
    } catch {
        throw "Legacy MCP quarantine cleanup failed; migration is incomplete: $($_.Exception.Message)"
    }
    $script:LegacyQuarantine = @()
}

function Restore-WebSwap {
    $oldWeb = Join-Path $WebTxnDir 'old-web'
    if ($script:WebSwapped -and (Test-Path -LiteralPath $WebDir)) {
        Assert-NormalDirectoryTree $WebDir 'Failed live web tree'
        Remove-Item -LiteralPath $WebDir -Recurse -Force -ErrorAction Stop
    }
    if ($script:WebBackedUp) {
        if (-not (Test-Path -LiteralPath $oldWeb -PathType Container)) {
            throw "Web backup is unavailable: $oldWeb"
        }
        if (Test-Path -LiteralPath $WebDir) { throw "Web restore target already exists: $WebDir" }
        Move-Item -LiteralPath $oldWeb -Destination $WebDir -ErrorAction Stop
        Assert-NormalDirectoryTree $WebDir 'Restored web tree'
    }
    if (-not $script:WebBackedUp -and (Test-Path -LiteralPath $WebDir)) {
        throw "Failed live web directory still exists: $WebDir"
    }
    $script:WebSwapped = $false
    $script:WebBackedUp = $false
}

function Restore-ServerSwap {
    $oldServer = Join-Path $ServerTxnDir 'old-server'
    if ($script:ServerSwapped -and (Test-Path -LiteralPath $DestBin)) {
        $live = Get-Item -LiteralPath $DestBin -Force -ErrorAction Stop
        if (-not ($live -is [IO.FileInfo]) -or $live.PSIsContainer `
            -or ($live.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Cannot safely remove the failed live server: $DestBin"
        }
        Remove-Item -LiteralPath $DestBin -Force -ErrorAction Stop
    }
    if ($script:ServerBackedUp) {
        $backup = Get-Item -LiteralPath $oldServer -Force -ErrorAction Stop
        if (-not ($backup -is [IO.FileInfo]) -or $backup.PSIsContainer `
            -or ($backup.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Server backup is not a regular file: $oldServer"
        }
        if (Test-Path -LiteralPath $DestBin) { throw "Server restore target already exists: $DestBin" }
        Move-Item -LiteralPath $oldServer -Destination $DestBin -ErrorAction Stop
        $restored = Get-Item -LiteralPath $DestBin -Force -ErrorAction Stop
        if (-not ($restored -is [IO.FileInfo]) -or $restored.PSIsContainer `
            -or ($restored.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Restored server is not a regular file: $DestBin"
        }
    }
    if (-not $script:ServerBackedUp -and (Test-Path -LiteralPath $DestBin)) {
        throw "Failed live server still exists: $DestBin"
    }
    $script:ServerSwapped = $false
    $script:ServerBackedUp = $false
}

function Rollback-InstallTransaction {
    $errors = @()
    try { Restore-WebSwap } catch { $errors += "web: $($_.Exception.Message)" }
    try { Restore-ServerSwap } catch { $errors += "server: $($_.Exception.Message)" }
    try { Restore-LegacyMcpQuarantine $InstallDir } catch { $errors += "legacy MCP: $($_.Exception.Message)" }
    if ($errors.Count -gt 0) { throw ($errors -join '; ') }
    $script:RollbackComplete = $true
}

function Test-InstallMutationStarted {
    return $script:ServerBackedUp -or $script:ServerSwapped `
        -or $script:WebBackedUp -or $script:WebSwapped `
        -or $script:LegacyQuarantine.Count -gt 0
}

function Commit-StagedBundle {
    $oldServer = Join-Path $ServerTxnDir 'old-server'
    if (Test-Path -LiteralPath $DestBin) {
        Move-Item -LiteralPath $DestBin -Destination $oldServer -ErrorAction Stop
        $script:ServerBackedUp = $true
    }
    Move-Item -LiteralPath (Join-Path $ServerTxnDir 'new-server.exe') -Destination $DestBin -ErrorAction Stop
    $script:ServerSwapped = $true
    $oldWeb = Join-Path $WebTxnDir 'old-web'
    if (Test-Path -LiteralPath $WebDir) {
        Move-Item -LiteralPath $WebDir -Destination $oldWeb -ErrorAction Stop
        $script:WebBackedUp = $true
    }
    Move-Item -LiteralPath (Join-Path $WebTxnDir 'new-web') -Destination $WebDir -ErrorAction Stop
    $script:WebSwapped = $true
}

function Write-PreservedTransactionPaths {
    Write-Host 'Transaction finalization is incomplete. These directories were retained for recovery inspection:'
    foreach ($path in @($script:ServerTxnDir, $script:WebTxnDir)) {
        if ($path) { Write-Host "  $path" }
    }
    Write-Host 'Do not delete these directories until server, web, and legacy MCP state are recovered.'
}

function Test-TransactionRecoveryPayload {
    try {
        if ($script:ServerTxnDir -and (Test-Path -LiteralPath (Join-Path $ServerTxnDir 'old-server'))) {
            return $true
        }
        if ($script:WebTxnDir -and (Test-Path -LiteralPath (Join-Path $WebTxnDir 'old-web'))) {
            return $true
        }
        if ($script:LegacyQuarantineDir -and (Test-Path -LiteralPath $LegacyQuarantineDir)) {
            return @(Get-ChildItem -LiteralPath $LegacyQuarantineDir -Force -ErrorAction Stop).Count -gt 0
        }
        return $false
    } catch {
        return $true
    }
}

function Cleanup-InstallerDirectories {
    if (-not $script:BundleCommitted -and $script:RollbackComplete `
        -and (Test-TransactionRecoveryPayload)) {
        $script:RollbackComplete = $false
        $script:ExitStatus = 1
        Write-Host 'Error: rollback verification found retained backup data.'
    }
    $mayDeleteTransactions = $script:BundleCommitted -or $script:RollbackComplete
    if ($mayDeleteTransactions) {
        $targets = @(
            [PSCustomObject]@{ Path = $script:ServerTxnDir; Parent = $InstallDir; Label = 'Server transaction' },
            [PSCustomObject]@{ Path = $script:WebTxnDir; Parent = (Split-Path -Parent $WebDir); Label = 'Web transaction' }
        )
        foreach ($target in $targets) {
            try {
                Remove-GeneratedDirectory $target.Path $target.Parent $target.Label
            } catch {
                $script:ExitStatus = 1
                Write-Host "Error: $($target.Label) cleanup failed; retained $($target.Path): $($_.Exception.Message)"
            }
        }
    } else {
        Write-PreservedTransactionPaths
    }
    try {
        Remove-GeneratedDirectory $TmpDir $safeTemp 'Temporary download'
    } catch {
        $script:ExitStatus = 1
        Write-Host "Error: temporary installer cleanup failed; retained ${TmpDir}: $($_.Exception.Message)"
    }
}

function Write-InstallationSuccess([string]$VersionText) {
    Write-Host ""
    Write-Host "HTTP-only iyw-claw-server installation completed."
    Write-Host "Binary: $InstallDir\iyw-claw-server.exe"
    Write-Host "Version: $VersionText"
    Write-Host ""
    Write-Host "Quick start:"
    Write-Host "  `$env:IYW_CLAW_STATIC_DIR=`"$WebDir`"; iyw-claw-server"
    Write-Host ""
    Write-Host "Or with custom settings:"
    Write-Host "  `$env:IYW_CLAW_PORT=`"3080`"; `$env:IYW_CLAW_TOKEN=`"your-secret`"; `$env:IYW_CLAW_STATIC_DIR=`"$WebDir`"; iyw-claw-server"
}

function Remove-PathConflicts([string[]]$Conflicts) {
    if (-not $Cleanup -or $Conflicts.Count -eq 0) { return }
    Write-Host ""
    Write-Host "Removing stale iyw-claw-server binaries..."
    foreach ($conflict in $Conflicts) {
        try {
            Remove-Item -LiteralPath $conflict -Force -ErrorAction Stop
            Write-Host "  removed $conflict"
        } catch {
            Write-Host "  failed to remove $conflict; remove it manually: $($_.Exception.Message)"
            $script:ExitStatus = 1
        }
    }
}

function Read-BinVersion([string]$BinPath) {
    if (-not (Test-Path -LiteralPath $BinPath)) { return "" }
    $stdout = Join-Path $env:TEMP ("iyw-claw-ver-" + [Guid]::NewGuid().ToString() + ".txt")
    $stderr = Join-Path $env:TEMP ("iyw-claw-vererr-" + [Guid]::NewGuid().ToString() + ".txt")
    try {
        $proc = Start-Process -FilePath $BinPath -ArgumentList "--version" `
            -NoNewWindow -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $exited = $proc.WaitForExit(3000)
        if (-not $exited) {
            try { $proc.Kill() } catch {}
            return ""
        }
        if ($proc.ExitCode -ne 0) { return "" }
        if (Test-Path $stdout) {
            $line = (Get-Content $stdout -ErrorAction SilentlyContinue | Select-Object -First 1)
            if ($line) { return $line.Trim() }
        }
        return ""
    } catch {
        return ""
    } finally {
        Remove-Item $stdout -Force -ErrorAction SilentlyContinue
        Remove-Item $stderr -Force -ErrorAction SilentlyContinue
    }
}

# ── Resolve version ──

if (-not $Version) {
    Write-Host "Fetching latest release..."
    $release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $release.tag_name
    if (-not $Version) {
        Write-Error "Could not determine latest version"
        exit 1
    }
}

if ($Version -notmatch $StableTagPattern) {
    Write-Error "Release tag '$Version' must be stable SemVer in the form vMAJOR.MINOR.PATCH."
    exit 1
}
$TargetVer = $Version.Substring(1)
if ([version]$TargetVer -lt $MinHttpOnlyVersion) {
    Write-Error "Release $Version predates the HTTP-only built-in MCP release. Install $MinHttpOnlyVersion or newer; the existing installation was not changed."
    exit 1
}
$Minisign = Get-Command minisign -CommandType Application -ErrorAction SilentlyContinue
if (-not $Minisign) {
    Write-Error "The 'minisign' command is required to verify the signed server archive. No installed file was changed."
    exit 1
}
Assert-TargetLayout

# ── Scan PATH for iyw-claw-server binaries that shadow the target install ──
#
# A binary "shadows" the install if it appears in PATH BEFORE the destination
# directory: that's the binary `Get-Command iyw-claw-server` returns after install.
# Unlike install.sh (which doesn't modify PATH), this script appends
# `$InstallDir` to user PATH below when it's missing, so any pre-existing
# iyw-claw-server in PATH ends up before the destination after install and must be
# cleaned. We therefore collect conflicts even when the destination isn't on
# PATH yet: stop the walk at the destination if present, otherwise scan to the
# end (post-install, the destination will be at the tail).

$InstallDirReal = Get-SafeCanonicalPath $InstallDir
$InstallDir = $InstallDirReal
$DestBin = Join-Path $InstallDir "iyw-claw-server.exe"
$WebDir = Join-Path $InstallDir "web"
$DestBinReal = Get-SafeCanonicalPath $DestBin
$null = Get-SafeCanonicalPath $WebDir

$PathConflicts = @()
$seenReal = @{}
$pathDirs = @()
if ($env:Path) { $pathDirs = $env:Path.Split(';') }
# Scan PATH for managed binaries that shadow the destination.
foreach ($dir in $pathDirs) {
    if (-not $dir) { continue }
    # Match by canonical path string so the destination is recognized even when
    # the directory doesn't exist yet (e.g. first install into a fresh prefix).
    $dirReal = Get-CanonicalPath $dir
    if ($dirReal -eq $InstallDirReal) {
        break
    }
    foreach ($name in $ManagedBins) {
        foreach ($leaf in @("$name.exe", $name)) {
            $bin = Join-Path $dir $leaf
            if (Test-Path -LiteralPath $bin -PathType Leaf) {
                $real = Get-CanonicalPath $bin
                if ($seenReal.ContainsKey($real)) { continue }
                $seenReal[$real] = $true
                $PathConflicts += $bin
            }
        }
    }
}

# What does `iyw-claw-server` actually resolve to in the current PATH?
$ActiveBin = ""
$resolved = Get-Command iyw-claw-server -ErrorAction SilentlyContinue
if ($resolved) { $ActiveBin = $resolved.Source }

# ── Version detection — prefer the binary the user actually invokes ──

$VersionCheckBin = ""
if ($ActiveBin -and (Test-Path -LiteralPath $ActiveBin)) {
    $VersionCheckBin = $ActiveBin
} elseif (Test-Path -LiteralPath $DestBin) {
    $VersionCheckBin = $DestBin
}

$CurrentVersion = ""
$WasRunning = $false
if ($VersionCheckBin) {
    $CurrentVersion = Read-BinVersion $VersionCheckBin
}
$LegacyCleanupRequired = @(Get-LegacyMcpFiles $InstallDir).Count -gt 0 `
    -or @(Get-LegacyMcpProcesses $InstallDir).Count -gt 0

# Only short-circuit when the active binary is up to date AND the destination
# itself has it, the web entry point is present, and no other PATH entries
# shadow it.
if ($CurrentVersion -and ($CurrentVersion -eq $TargetVer) `
        -and ($PathConflicts.Count -eq 0) `
        -and (-not $LegacyCleanupRequired) `
        -and (Test-NonEmptyFile $DestBin) `
        -and (Test-NonEmptyFile (Join-Path $WebDir "index.html"))) {
    Write-Host "iyw-claw-server is already at version $TargetVer with web assets in place, nothing to do."
    exit 0
}

if ($CurrentVersion) {
    Write-Host "Upgrading iyw-claw-server: $CurrentVersion -> $TargetVer..."
} else {
    Write-Host "Installing iyw-claw-server $Version (windows/x64)..."
}

# ── Warn about iyw-claw-server binaries shadowing the target install ──

if ($PathConflicts.Count -gt 0) {
    Write-Host ""
    Write-Host "Found other iyw-claw-server binaries in PATH that may shadow ${DestBin}:"
    foreach ($c in $PathConflicts) {
        $cv = Read-BinVersion $c
        if ($cv) {
            Write-Host "  - $c  (version $cv)"
        } else {
            Write-Host "  - $c"
        }
    }
    if ($Cleanup) {
        Write-Host "These will be removed after installation. Pass -NoCleanup to keep them."
    } else {
        Write-Host "Keeping them (-NoCleanup). You may need to remove them manually so that"
        Write-Host "typing 'iyw-claw-server' runs the new install at $DestBin."
    }
    Write-Host ""
}

# ── Download and extract ──

$Url = "https://github.com/$Repo/releases/download/$Version/$Artifact.zip"
$SignatureUrl = "$Url.sig"
$safeTemp = Get-SafeCanonicalPath $env:TEMP
$TmpDir = Join-Path $safeTemp ("iyw-claw-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $TmpDir | Out-Null
$null = Get-SafeCanonicalPath $TmpDir
$ZipPath = Join-Path $TmpDir "$Artifact.zip"
$SignaturePath = "$ZipPath.sig"
$MiniSigPath = Join-Path $TmpDir "$Artifact.minisig"
$script:ServerTxnDir = ""
$script:WebTxnDir = ""
$script:ServerBackedUp = $false
$script:ServerSwapped = $false
$script:WebBackedUp = $false
$script:WebSwapped = $false
$script:BundleCommitted = $false
$script:LiveBundleVerified = $false
$script:RollbackComplete = $false
$script:LegacyQuarantineDir = ""
$script:LegacyQuarantine = @()
$script:ExitStatus = 1

try {

Write-Host "Downloading $Url..."
try {
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
} catch {
    throw "Download failed. Check that version $Version exists and has a $Artifact asset."
}
Assert-ArchiveFile $ZipPath
try {
    Invoke-WebRequest -Uri $SignatureUrl -OutFile $SignaturePath -UseBasicParsing
} catch {
    throw 'Detached signature download failed for the same fixed release tag.'
}
if ((Get-Item -LiteralPath $SignaturePath).Length -gt 16384) {
    throw 'Detached signature is unexpectedly large.'
}
try {
    $encoded = (Get-Content -LiteralPath $SignaturePath -Raw).Trim()
    $decoded = [Convert]::FromBase64String($encoded)
    [IO.File]::WriteAllBytes($MiniSigPath, $decoded)
} catch {
    throw "Detached signature is not valid Tauri base64-wrapped minisign text: $($_.Exception.Message)"
}
& $Minisign.Source -Vm $ZipPath -x $MiniSigPath -P $MinisignPublicKey
if ($LASTEXITCODE -ne 0) {
    throw 'Release archive signature verification failed.'
}

Get-ZipInventory $ZipPath

Write-Host "Extracting..."
Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force
Assert-ExtractedBundle (Join-Path $TmpDir $Artifact)

Prepare-TargetStaging (Join-Path $TmpDir $Artifact)

$ServerProcesses = @(Get-ScopedProcessSnapshots $InstallDir 'server')
if ($ServerProcesses.Count -gt 0) {
    Write-Host "Stopping running iyw-claw-server process(es)..."
    $WasRunning = $true
}
if (-not (Stop-ScopedProcesses $InstallDir 'server' 10)) {
    throw "Could not stop iyw-claw-server from $InstallDir."
}
if ($WasRunning) {
    Write-Host "iyw-claw-server stopped."
}

if (@(Get-LegacyMcpFiles $InstallDir).Count -gt 0 `
        -or @(Get-LegacyMcpProcesses $InstallDir).Count -gt 0) {
    Write-Host "Migrating legacy iyw-claw-mcp files from $InstallDir..."
}
if (-not (Stop-LegacyMcpProcesses $InstallDir)) {
    throw 'Legacy MCP process cleanup failed; migration is incomplete'
}
Move-LegacyMcpToQuarantine $InstallDir

Commit-StagedBundle
$InstalledVer = Read-BinVersion $DestBin
if ($InstalledVer -ne $TargetVer -or -not (Test-NonEmptyFile $DestBin) `
    -or -not (Test-NonEmptyFile (Join-Path $WebDir 'index.html'))) {
    throw 'Live server/web verification failed'
}
$script:LiveBundleVerified = $true

if (-not (Stop-LegacyMcpProcesses $InstallDir)) {
    throw 'Live bundle is valid, but legacy MCP process cleanup failed; migration is incomplete'
}
Move-LegacyMcpToQuarantine $InstallDir
if (@(Get-LegacyMcpFiles $InstallDir).Count -gt 0 `
        -or @(Get-LegacyMcpProcesses $InstallDir).Count -gt 0) {
    throw "Legacy MCP files or processes remain in $InstallDir; migration is incomplete"
}
Remove-LegacyMcpQuarantine
if (@(Get-LegacyMcpFiles $InstallDir).Count -gt 0 `
        -or @(Get-LegacyMcpProcesses $InstallDir).Count -gt 0) {
    throw "Legacy MCP files or processes reappeared in $InstallDir; migration is incomplete"
}
$script:BundleCommitted = $true
$script:ExitStatus = 0

# ── Add to PATH ──

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to user PATH (restart terminal to take effect)"
}
# Mirror the change into the current process so the post-install verification
# below can resolve `iyw-claw-server`. Without this, the first-time install would
# always exit non-zero on Windows because Get-Command runs against the in-process
# $env:Path that does not yet include $InstallDir.
if ($env:Path -notlike "*$InstallDir*") {
    $env:Path = "$env:Path;$InstallDir"
}

# ── Remove shadowing binaries from earlier PATH entries ──

Remove-PathConflicts $PathConflicts

# ── Restart service if it was running ──

if ($WasRunning) {
    Write-Host ""
    Write-Host "Note: iyw-claw-server was stopped for the upgrade."
    Write-Host "Please restart it manually to ensure your environment variables (IYW_CLAW_PORT, IYW_CLAW_TOKEN, etc.) are preserved:"
    Write-Host "  `$env:IYW_CLAW_STATIC_DIR=`"$WebDir`"; iyw-claw-server"
}

# ── Done ──

$InstalledVer = ""
try {
    $InstalledOutput = & (Join-Path $InstallDir "iyw-claw-server.exe") --version 2>$null
    $InstalledExitCode = $LASTEXITCODE
    if ($InstalledExitCode -eq 0 -and $InstalledOutput) {
        $InstalledVer = ($InstalledOutput | Select-Object -First 1).Trim()
    }
} catch {}
if (-not $InstalledVer) {
    Write-Host "Error: installed iyw-claw-server.exe failed its --version check."
    $script:ExitStatus = 1
} elseif ($InstalledVer -ne $TargetVer) {
    Write-Host "Error: installed iyw-claw-server.exe version is $InstalledVer; expected $TargetVer."
    $script:ExitStatus = 1
}

# Verify the user's `iyw-claw-server` command actually resolves to the new binary.
$ActiveBinAfter = ""
$resolvedAfter = Get-Command iyw-claw-server -ErrorAction SilentlyContinue
if ($resolvedAfter) { $ActiveBinAfter = $resolvedAfter.Source }
$ActiveBinAfterReal = Get-CanonicalPath $ActiveBinAfter

if (-not $ActiveBinAfter) {
    Write-Host ""
    Write-Host "Note: $InstallDir is not on the current session's PATH."
    Write-Host "Open a new terminal (PATH was just updated) or run:"
    Write-Host "  `$env:Path = `"$InstallDir;`$env:Path`""
    $script:ExitStatus = 1
} elseif ($ActiveBinAfterReal -ne $DestBinReal) {
    Write-Host ""
    Write-Host "Warning: typing 'iyw-claw-server' still runs $ActiveBinAfter, not $DestBin."
    Write-Host "Another binary earlier in PATH is shadowing the new install. To fix, either:"
    Write-Host "  - re-run without -NoCleanup (the default removes shadowing binaries), or"
    Write-Host "  - remove the stale binary manually: Remove-Item '$ActiveBinAfter', or"
    Write-Host "  - put $InstallDir before its directory in PATH."
    $script:ExitStatus = 1
}

} catch {
    $failureMessage = $_.Exception.Message
    $script:ExitStatus = 1
    if (-not $script:BundleCommitted -and -not $script:LiveBundleVerified) {
        if (Test-InstallMutationStarted) {
            try {
                Rollback-InstallTransaction
                Write-Host 'Installation transaction was rolled back; previous files were restored.'
            } catch {
                Write-Host "Error: automatic rollback failed: $($_.Exception.Message)"
            }
        } else {
            $script:RollbackComplete = $true
        }
    }
    if ($script:BundleCommitted) {
        Write-Host "Error: HTTP-only bundle is installed, but post-install setup failed: $failureMessage"
    } elseif ($script:LiveBundleVerified) {
        Write-Host "Error: HTTP-only bundle passed live verification, but migration is incomplete: $failureMessage"
    } else {
        Write-Host "Error: HTTP-only installation failed: $failureMessage"
    }
} finally {
    Cleanup-InstallerDirectories
}

if ($script:ExitStatus -eq 0) {
    Write-InstallationSuccess $InstalledVer
} elseif ($script:BundleCommitted) {
    Write-Host "HTTP-only bundle remains installed at $DestBin, but installation did not complete cleanly."
    Write-Host 'Review the errors above before starting the server.'
}

exit $script:ExitStatus

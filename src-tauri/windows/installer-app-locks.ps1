param(
    [Parameter(Mandatory = $true)][string]$AppDirectory,
    [switch]$ReleaseExplorer
)

$ErrorActionPreference = 'Stop'
$navigationTimeoutMs = 6000
$navigationPollMs = 200
$shell = $null
$windows = $null
$pending = [Collections.Generic.List[object]]::new()

function Release-ComReference($Value) {
    if ($null -ne $Value -and [Runtime.InteropServices.Marshal]::IsComObject($Value)) {
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($Value)
    }
}

function Get-ExplorerPath($Window) {
    $document = $null
    $folder = $null
    $item = $null
    try {
        $document = $Window.Document
        $folder = $document.Folder
        $item = $folder.Self
        $path = $item.Path
        if ([string]::IsNullOrWhiteSpace($path) -or -not [IO.Path]::IsPathRooted($path)) { return $null }
        return [IO.Path]::GetFullPath($path).TrimEnd('\')
    } finally {
        Release-ComReference $item
        Release-ComReference $folder
        Release-ComReference $document
    }
}

function Test-AppPath([string]$Path) {
    return $Path -and ($Path -ieq $root -or $Path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase))
}

function Wait-ExplorerNavigation {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    do {
        $remaining = 0
        foreach ($entry in $pending) {
            if ($entry.Confirmed) { continue }
            try {
                $path = Get-ExplorerPath $entry.Window
                if ($path -and -not (Test-AppPath $path) -and -not $entry.Window.Busy) {
                    $entry.Confirmed = $true
                    Write-Output "Explorer navigation confirmed: source=$($entry.Source); current=$path"
                    continue
                }
            } catch {
                $entry.ErrorCode = $_.Exception.HResult
            }
            $remaining++
        }
        if ($remaining -eq 0 -or $watch.ElapsedMilliseconds -ge $navigationTimeoutMs) { break }
        Start-Sleep -Milliseconds $navigationPollMs
    } while ($true)
    foreach ($entry in $pending) {
        if (-not $entry.Confirmed) {
            Write-Output "Explorer navigation not confirmed before timeout: source=$($entry.Source); hresult=$($entry.ErrorCode)"
        }
    }
    if ($pending.Count -gt 0) {
        Write-Output 'Explorer inspection finished; the installer will verify directory availability by retrying the atomic backup.'
    }
}

try {
    $root = [IO.Path]::GetFullPath($AppDirectory).TrimEnd('\')
    $prefix = $root + '\'
    $destination = [IO.Path]::GetDirectoryName($root)
    $shell = New-Object -ComObject Shell.Application
    $windows = $shell.Windows()
    $found = $false
    foreach ($window in $windows) {
        $retained = $false
        try {
            if ([IO.Path]::GetFileName($window.FullName) -ine 'explorer.exe') { continue }
            $path = Get-ExplorerPath $window
            if (-not (Test-AppPath $path)) { continue }
            $found = $true
            Write-Output "Open Explorer folder under app: $path"
            if ($ReleaseExplorer) {
                $window.Navigate2($destination)
                Write-Output "Explorer navigation requested: source=$path; destination=$destination"
                $pending.Add([pscustomobject]@{ Window = $window; Source = $path; Confirmed = $false; ErrorCode = 0 })
                $retained = $true
            }
        } catch {
            Write-Output "Explorer window inspection or navigation failed: hresult=$($_.Exception.HResult)"
        } finally {
            if (-not $retained) { Release-ComReference $window }
        }
    }
    if ($ReleaseExplorer) { Wait-ExplorerNavigation }
    if (-not $found) {
        Write-Output 'No open Explorer folder found. A terminal, file preview, another process or directory permissions may still prevent renaming app.'
    }
} catch {
    Write-Output "Explorer directory inspection failed: $($_.Exception.Message)"
} finally {
    foreach ($entry in $pending) { Release-ComReference $entry.Window }
    Release-ComReference $windows
    Release-ComReference $shell
}

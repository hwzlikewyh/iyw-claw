param(
    [Parameter(Mandatory = $true)][string]$AppDirectory,
    [switch]$ReleaseExplorer
)

$ErrorActionPreference = 'Stop'
$shell = $null
$windows = $null
try {
    $root = [IO.Path]::GetFullPath($AppDirectory).TrimEnd('\')
    $prefix = $root + '\'
    $destination = [IO.Path]::GetDirectoryName($root)
    $shell = New-Object -ComObject Shell.Application
    $windows = $shell.Windows()
    $found = $false
    foreach ($window in $windows) {
        try {
            if ([IO.Path]::GetFileName($window.FullName) -ine 'explorer.exe') { continue }
            $path = $window.Document.Folder.Self.Path
            if (-not [IO.Path]::IsPathRooted($path)) { continue }
            $path = [IO.Path]::GetFullPath($path).TrimEnd('\')
            if ($path -ine $root -and -not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { continue }
            Write-Output "Open Explorer folder under app: $path"
            if ($ReleaseExplorer) {
                $window.Navigate2($destination)
                Write-Output "Navigated Explorer outside app: $path"
            }
            $found = $true
        } catch {
            # A closed or non-filesystem window must not block recovery.
        } finally {
            if ($null -ne $window) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($window) }
        }
    }
    if (-not $found) {
        Write-Output 'No open Explorer folder found. A terminal, file preview, another process or directory permissions may still prevent renaming app.'
    }
} catch {
    Write-Output "Explorer directory inspection failed: $($_.Exception.Message)"
} finally {
    if ($null -ne $windows) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($windows) }
    if ($null -ne $shell) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell) }
}

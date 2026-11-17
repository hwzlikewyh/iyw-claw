$shareScales = @(100, 125, 150, 200, 400)
$shareTargetSizes = @(16, 24, 32, 48, 256)
$pngHeaderBytes = 24
$pngWidthOffset = 16
$pngHeightOffset = 20
$pngSignatureBytes = 8
function Get-ShareAssetSpecs {
    foreach ($size in @(44, 150)) {
        $name = "Square${size}x${size}Logo"
        [pscustomobject]@{ Name = "$name.png"; Size = $size }
        foreach ($scale in $shareScales) { [pscustomobject]@{ Name = "$name.scale-$scale.png"; Size = [int][Math]::Round($size * $scale / 100) } }
    }
    foreach ($size in $shareTargetSizes) { foreach ($suffix in @('', '_altform-unplated')) { [pscustomobject]@{ Name = "Square44x44Logo.targetsize-$size$suffix.png"; Size = $size } } }
}
function Get-ShareResourceNames {
    'resources.pri'
    foreach ($scale in $shareScales | Where-Object { $_ -ne 100 }) { "resources.scale-$scale.pri" }
    Get-ShareAssetSpecs | ForEach-Object { "Assets/$($_.Name)" }
}
function Assert-ShareResources([string]$Directory) {
    foreach ($name in Get-ShareResourceNames) { $file = Get-Item -LiteralPath (Join-Path $Directory $name) -ErrorAction Stop; if ($file.PSIsContainer -or $file.Length -eq 0) { throw "Share resource is empty: $name" } }
    foreach ($asset in Get-ShareAssetSpecs) {
        $stream = [IO.File]::OpenRead((Join-Path $Directory "Assets/$($asset.Name)"))
        try {
            $header = New-Object byte[] $pngHeaderBytes
            if ($stream.Read($header, 0, $header.Length) -ne $header.Length -or [BitConverter]::ToString($header, 0, $pngSignatureBytes) -ne '89-50-4E-47-0D-0A-1A-0A') { throw "Share image is not a PNG: $($asset.Name)" }
            $width = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($header, $pngWidthOffset)); $height = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($header, $pngHeightOffset))
            if ($width -ne $asset.Size -or $height -ne $asset.Size) { throw "Share image size mismatch: $($asset.Name)" }
        } finally { $stream.Dispose() }
    }
}
function Assert-ShareResourceDestination([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        $entry = Get-Item -LiteralPath $Path -Force
        if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Share resource destination must not be a link: $Path" }
    }
}
function Update-ShareShellIcons {
    if (-not ('IywShareShell' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class IywShareShell {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(uint events, uint flags, IntPtr item1, IntPtr item2);
}
'@
    }
    $associationChanged = 0x08000000
    $idList = 0
    [IywShareShell]::SHChangeNotify($associationChanged, $idList, [IntPtr]::Zero, [IntPtr]::Zero)
}
function Sync-ShareExternalResources([string]$Source, [string]$Destination, [switch]$CheckOnly) {
    # 稀疏身份包的 VisualElements 相对于 ExternalLocation，不能只校验 WindowsApps 内的副本。
    Assert-ShareResources $Source
    Assert-ShareResourceDestination $Destination
    Assert-ShareResourceDestination (Join-Path $Destination 'Assets')
    $copied = 0
    foreach ($name in Get-ShareResourceNames) {
        $sourceFile = Join-Path $Source $name
        $destinationFile = Join-Path $Destination $name
        Assert-ShareResourceDestination $destinationFile
        $sourceHash = (Get-FileHash -LiteralPath $sourceFile -Algorithm SHA256).Hash
        if ((Test-Path -LiteralPath $destinationFile -PathType Leaf) -and
            (Get-FileHash -LiteralPath $destinationFile -Algorithm SHA256).Hash -eq $sourceHash) { continue }
        if ($CheckOnly) { return $false }
        $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destinationFile) -Force
        Copy-Item -LiteralPath $sourceFile -Destination $destinationFile -Force
        if ((Get-FileHash -LiteralPath $destinationFile -Algorithm SHA256).Hash -ne $sourceHash) { throw "External share resource copy mismatch: $name" }
        $copied++
    }
    if ($copied -gt 0) {
        Update-ShareShellIcons
        Write-Host "Restored external shell icon resources: files=$copied"
    }
    return $true
}

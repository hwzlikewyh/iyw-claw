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
        foreach ($scale in $shareScales) {
            [pscustomobject]@{
                Name = "$name.scale-$scale.png"
                Size = [int][Math]::Round($size * $scale / 100)
            }
        }
    }
    foreach ($size in $shareTargetSizes) {
        foreach ($suffix in @('', '_altform-unplated')) {
            [pscustomobject]@{ Name = "Square44x44Logo.targetsize-$size$suffix.png"; Size = $size }
        }
    }
}

function Get-ShareResourceNames {
    'resources.pri'
    foreach ($scale in $shareScales | Where-Object { $_ -ne 100 }) { "resources.scale-$scale.pri" }
    Get-ShareAssetSpecs | ForEach-Object { "Assets/$($_.Name)" }
}

function Assert-ShareResources([string]$Directory) {
    foreach ($name in Get-ShareResourceNames) {
        $file = Get-Item -LiteralPath (Join-Path $Directory $name) -ErrorAction Stop
        if ($file.PSIsContainer -or $file.Length -eq 0) { throw "Share resource is empty: $name" }
    }
    foreach ($asset in Get-ShareAssetSpecs) {
        $path = Join-Path $Directory "Assets/$($asset.Name)"
        $stream = [IO.File]::OpenRead($path)
        try {
            $header = New-Object byte[] $pngHeaderBytes
            if ($stream.Read($header, 0, $header.Length) -ne $header.Length -or
                [BitConverter]::ToString($header, 0, $pngSignatureBytes) -ne '89-50-4E-47-0D-0A-1A-0A') {
                throw "Share image is not a PNG: $($asset.Name)"
            }
            $width = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($header, $pngWidthOffset))
            $height = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($header, $pngHeightOffset))
            if ($width -ne $asset.Size -or $height -ne $asset.Size) { throw "Share image size mismatch: $($asset.Name)" }
        } finally { $stream.Dispose() }
    }
}

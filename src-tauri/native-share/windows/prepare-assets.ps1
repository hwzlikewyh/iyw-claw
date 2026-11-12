param([Parameter(Mandatory=$true)][string]$Source, [Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
. (Join-Path $PSScriptRoot 'share-resources.ps1')
New-Item -ItemType Directory -Path $Output -Force | Out-Null
$image = [Drawing.Image]::FromFile($Source)
try {
    foreach ($asset in Get-ShareAssetSpecs) {
        $bitmap = New-Object Drawing.Bitmap($asset.Size, $asset.Size)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($image, 0, 0, $asset.Size, $asset.Size)
            $bitmap.Save((Join-Path $Output $asset.Name), [Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
} finally { $image.Dispose() }

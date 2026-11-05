param([Parameter(Mandatory=$true)][string]$Source, [Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Path $Output -Force | Out-Null
$image = [Drawing.Image]::FromFile($Source)
try {
    foreach ($size in @(44, 150)) {
        $bitmap = New-Object Drawing.Bitmap($size, $size)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($image, 0, 0, $size, $size)
            $bitmap.Save((Join-Path $Output "Square${size}x${size}Logo.png"), [Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
} finally { $image.Dispose() }

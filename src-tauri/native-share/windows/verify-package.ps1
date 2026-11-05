param([Parameter(Mandatory=$true)][string]$Directory, [Parameter(Mandatory=$true)][string]$ExecutableManifest)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead((Join-Path $Directory 'IywAssistant.ShareTarget.msix'))
try {
    $entry = $archive.GetEntry('AppxManifest.xml')
    if (-not $entry) { throw 'MSIX manifest missing' }
    $reader = New-Object IO.StreamReader($entry.Open())
    try { [xml]$package = $reader.ReadToEnd() } finally { $reader.Dispose() }
    [xml]$executable = Get-Content -LiteralPath $ExecutableManifest -Encoding UTF8 -Raw
    $identity = $package.Package.Identity
    $application = $package.Package.Applications.Application
    $msix = $executable.assembly.msix
    if ($identity.Name -ne 'IywAssistant.ShareTarget' -or $application.Id -ne 'Share' -or
        $msix.packageName -ne $identity.Name -or $msix.publisher -ne $identity.Publisher -or
        $msix.applicationId -ne $application.Id -or $application.Executable -ne 'iyw-share-target.exe') {
        throw 'Share receiver EXE identity does not match the MSIX manifest'
    }
    $config = Get-Content -LiteralPath (Join-Path $Directory 'config.json') -Encoding UTF8 -Raw | ConvertFrom-Json
    if ($identity.Version -ne $config.packageVersion) { throw 'Share package version mismatch' }
    foreach ($name in @('resources.pri', 'Assets/Square44x44Logo.png', 'Assets/Square150x150Logo.png')) {
        $entry = $archive.GetEntry($name)
        if (-not $entry -or $entry.Length -eq 0) { throw "Share package resource missing: $name" }
        $stream = $entry.Open()
        $hash = [Security.Cryptography.SHA256]::Create()
        try { $digest = [BitConverter]::ToString($hash.ComputeHash($stream)).Replace('-', '') }
        finally { $stream.Dispose(); $hash.Dispose() }
        if ($digest -ne (Get-FileHash -LiteralPath (Join-Path $Directory $name) -Algorithm SHA256).Hash) {
            throw "External share resource does not match the MSIX: $name"
        }
    }
} finally { $archive.Dispose() }
Write-Output 'Share package identity, version and external resources verified.'

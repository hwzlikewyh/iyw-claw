param([Parameter(Mandatory=$true)][string]$Directory, [Parameter(Mandatory=$true)][string]$ExecutableManifest)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
. (Join-Path $PSScriptRoot 'share-resources.ps1')
Assert-ShareResources $Directory
$archive = [IO.Compression.ZipFile]::OpenRead((Join-Path $Directory 'IywAssistant.ShareTarget.msix'))
try {
    $reader = New-Object IO.StreamReader($archive.GetEntry('AppxManifest.xml').Open())
    try { [xml]$package = $reader.ReadToEnd() } finally { $reader.Dispose() }
    [xml]$executable = Get-Content -LiteralPath $ExecutableManifest -Encoding UTF8 -Raw
    $identity = $package.Package.Identity; $application = $package.Package.Applications.Application; $msix = $executable.assembly.msix
    if ($identity.Name -ne 'IywAssistant.ShareTarget' -or $application.Id -ne 'Share.Hub' -or $msix.packageName -ne $identity.Name -or $msix.publisher -ne $identity.Publisher -or $msix.applicationId -ne $application.Id -or $application.Executable -ne 'iyw-share-target.exe') { throw 'Share receiver EXE identity does not match the MSIX manifest' }
    $config = Get-Content -LiteralPath (Join-Path $Directory 'config.json') -Encoding UTF8 -Raw | ConvertFrom-Json
    if ($identity.Version -ne $config.packageVersion) { throw 'Share package version mismatch' }
    $namespaces = New-Object Xml.XmlNamespaceManager($package.NameTable); $namespaces.AddNamespace('uap', 'http://schemas.microsoft.com/appx/manifest/uap/windows10')
    $shareTarget = $application.SelectSingleNode('./*[local-name()="Extensions"]/uap:Extension[@Category="windows.shareTarget"]/uap:ShareTarget', $namespaces)
    if (-not $shareTarget -or -not $shareTarget.SelectSingleNode('uap:SupportedFileTypes/uap:SupportsAnyFileType', $namespaces)) { throw 'Share package must accept any file type for host enumeration' }
    foreach ($format in @('StorageItems', 'Text', 'URI', 'Bitmap')) { if (-not $shareTarget.SelectSingleNode("uap:DataFormat[text()='$format']", $namespaces)) { throw "Share format missing: $format" } }
    foreach ($name in Get-ShareResourceNames) {
        $entry = $archive.GetEntry($name); if (-not $entry -or $entry.Length -eq 0) { throw "Share package resource missing: $name" }
        $stream = $entry.Open(); $hash = [Security.Cryptography.SHA256]::Create()
        try { $digest = [BitConverter]::ToString($hash.ComputeHash($stream)).Replace('-', '') } finally { $stream.Dispose(); $hash.Dispose() }
        if ($digest -ne (Get-FileHash -LiteralPath (Join-Path $Directory $name) -Algorithm SHA256).Hash) { throw "External share resource does not match the MSIX: $name" }
    }
} finally { $archive.Dispose() }
Write-Output 'Share package identity, version and external resources verified.'

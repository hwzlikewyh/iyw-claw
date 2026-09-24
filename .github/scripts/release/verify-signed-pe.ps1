param([string]$ExpectedRoot, [string]$SignedRoot, [string]$Thumbprint)
$ErrorActionPreference = 'Stop'

function Get-UnsignedPayload([string]$Path, [switch]$ForNsis) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($ForNsis) {
        # Tauri 将编译产物的唯一 UNK 标记改成 NSS，再执行签名。
        $marker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
        $text = [Text.Encoding]::ASCII.GetString($bytes)
        $offset = $text.IndexOf($marker, [StringComparison]::Ordinal)
        if ($offset -ge 0) {
            if ($text.IndexOf($marker, $offset + $marker.Length, [StringComparison]::Ordinal) -ge 0) {
                throw 'Ambiguous Tauri bundle marker'
            }
            [Array]::Copy([Text.Encoding]::ASCII.GetBytes('NSS'), 0, $bytes, $offset + $marker.Length - 3, 3)
        }
        $text = $null
    }
    $stream = [IO.MemoryStream]::new($bytes, $false)
    $reader = [System.Reflection.PortableExecutable.PEReader]::new($stream)
    try {
        $optional = $reader.PEHeaders.PEHeaderStartOffset
        $header = $reader.PEHeaders.PEHeader
        $certificate = $header.CertificateTableDirectory
        $directoryOffset = if ($header.Magic -eq [System.Reflection.PortableExecutable.PEMagic]::PE32Plus) { 144 } else { 128 }
        $length = $bytes.Length
        if ($certificate.Size -gt 0) {
            if ($certificate.RelativeVirtualAddress + $certificate.Size -ne $bytes.Length) { throw 'Unexpected data after the PE certificate' }
            $length = $certificate.RelativeVirtualAddress
        }
        # Authenticode 只允许改变校验和、证书目录和文件末尾的证书。
        [Array]::Clear($bytes, $optional + 64, 4)
        [Array]::Clear($bytes, $optional + $directoryOffset, 8)
        $sha = [Security.Cryptography.SHA256]::Create()
        try { return @{Length=$length; Hash=[Convert]::ToHexString($sha.ComputeHash($bytes, 0, $length))} }
        finally { $sha.Dispose() }
    } finally { $reader.Dispose(); $stream.Dispose() }
}

foreach ($name in @('iyw-claw.exe')) {
    $expected = Join-Path $ExpectedRoot $name
    $signed = Join-Path $SignedRoot $name
    $signature = Get-AuthenticodeSignature -LiteralPath $signed
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $Thumbprint) { throw "Invalid application signature: $name" }
    $before = Get-UnsignedPayload $expected -ForNsis
    $after = Get-UnsignedPayload $signed
    if ($before.Length -ne $after.Length -or $before.Hash -ne $after.Hash) { throw "Application changed beyond Authenticode metadata: $name" }
    Copy-Item -LiteralPath $signed -Destination $expected -Force
    Write-Output "Verified unchanged PE payload and expected signer: $name"
}

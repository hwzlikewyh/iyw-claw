param([string]$ExpectedRoot, [string]$SignedRoot, [string]$Thumbprint)
$ErrorActionPreference = 'Stop'

function Get-UnsignedPayload([string]$Path) {
    $bytes = [IO.File]::ReadAllBytes($Path)
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

foreach ($name in @('iyw-xinghe-helper.exe','xinghe-command-runner.exe','xinghe-windows-sandbox-setup.exe','iyw_xinghe_worker.dll')) {
    $expected = Join-Path $ExpectedRoot $name
    $signed = Join-Path $SignedRoot $name
    $signature = Get-AuthenticodeSignature -LiteralPath $signed
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $Thumbprint) { throw "Invalid helper signature: $name" }
    $before = Get-UnsignedPayload $expected
    $after = Get-UnsignedPayload $signed
    if ($before.Length -ne $after.Length -or $before.Hash -ne $after.Hash) { throw "Helper program changed beyond Authenticode metadata: $name" }
    Copy-Item -LiteralPath $signed -Destination $expected -Force
    Write-Output "Verified unchanged PE payload and expected signer: $name"
}

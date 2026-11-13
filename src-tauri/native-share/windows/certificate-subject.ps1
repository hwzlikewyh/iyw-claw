$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$mode = $env:IYW_CLAW_SIGN_MODE
if ($mode -eq 'signtool') {
    $thumbprint = ($env:IYW_CLAW_SIGN_THUMBPRINT -replace '[\s:]', '')
    if ($thumbprint -notmatch '^[A-Fa-f0-9]{40}$') { throw '签名证书指纹无效。' }
    foreach ($location in @('CurrentUser', 'LocalMachine')) {
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store(
            'My', [System.Security.Cryptography.X509Certificates.StoreLocation]$location)
        $certificates = $null
        try {
            $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly -bor
                [System.Security.Cryptography.X509Certificates.OpenFlags]::OpenExistingOnly)
            $certificates = $store.Certificates
            $matchingCertificates = @($certificates | Where-Object { $_.Thumbprint -eq $thumbprint })
            if ($matchingCertificates.Count -gt 1) { throw "签名证书在 $location/My 中不唯一。" }
            if ($matchingCertificates.Count -eq 1) { Write-Output $matchingCertificates[0].Subject; exit 0 }
        } finally {
            foreach ($certificate in $certificates) { $certificate.Dispose() }
            $store.Dispose()
        }
    }
    throw '无法在证书存储中找到当前签名证书。'
}
if ($mode -eq 'pfx') {
    $certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2(
        $env:IYW_CLAW_SIGN_PFX, $env:IYW_CLAW_SIGN_PFX_PASSWORD)
    Write-Output $certificate.Subject
    $certificate.Dispose()
    exit 0
}
throw '此签名方式请通过 IYW_SHARE_PUBLISHER 提供与证书完全一致的 MSIX Publisher。'

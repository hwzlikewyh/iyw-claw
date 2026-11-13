$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$mode = $env:IYW_CLAW_SIGN_MODE
if ($mode -eq 'signtool') {
    $thumbprint = ($env:IYW_CLAW_SIGN_THUMBPRINT -replace '[\s:]', '')
    if ($thumbprint -notmatch '^[A-Fa-f0-9]{40}$') { throw '签名证书指纹无效。' }
    foreach ($location in @(
        [Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser,
        [Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine
    )) {
        $store = New-Object Security.Cryptography.X509Certificates.X509Store(
            'My', $location
        )
        try {
            $store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly)
            $certificate = $store.Certificates | Where-Object {
                ($_.Thumbprint -replace '[\s:]', '') -eq $thumbprint
            } | Select-Object -First 1
            if ($certificate) { Write-Output $certificate.Subject; exit 0 }
        } finally {
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

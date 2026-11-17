$ErrorActionPreference = 'Stop'
$toolchain = 'nightly-2026-04-15'
$rustup = Get-Command rustup -ErrorAction Stop

function Test-PinnedWin7Toolchain {
    $sysroot = & $rustup.Source run $toolchain rustc --print sysroot 2>$null
    if ($LASTEXITCODE -ne 0 -or !$sysroot) { return $false }
    $source = Join-Path ($sysroot | Select-Object -Last 1) 'lib/rustlib/src/rust/library/std/Cargo.toml'
    if (!(Test-Path -LiteralPath $source -PathType Leaf)) { return $false }
    & $rustup.Source run $toolchain cargo --version 2>$null | Out-Null
    return $LASTEXITCODE -eq 0
}

if (Test-PinnedWin7Toolchain) {
    Write-Output "Using installed $toolchain with rust-src"
    exit 0
}

$servers = @('https://static.rust-lang.org', 'https://rsproxy.cn')
foreach ($server in $servers) {
    $env:RUSTUP_DIST_SERVER = $server
    $env:RUSTUP_UPDATE_ROOT = "$server/rustup"
    for ($attempt = 1; $attempt -le 2; $attempt++) {
        Write-Output "Installing $toolchain with rust-src from $server (attempt $attempt/2)"
        & $rustup.Source toolchain install $toolchain --profile minimal --component rust-src --no-self-update
        if ($LASTEXITCODE -eq 0 -and (Test-PinnedWin7Toolchain)) { exit 0 }
        if ($attempt -lt 2) { Start-Sleep -Seconds 3 }
    }
}
throw "Unable to install or verify $toolchain and rust-src from either Rust distribution server"

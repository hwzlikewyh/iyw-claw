$ErrorActionPreference = 'Stop'
$toolchain = '1.99.1'
$target = 'x86_64-pc-windows-msvc'
$rustup = (Get-Command rustup -ErrorAction Stop).Source

function Test-SigningToolchain {
    $directory = & $rustup run $toolchain rustc --print target-libdir --target $target 2>$null
    if ($LASTEXITCODE -ne 0 -or !$directory) { return $false }
    $libraries = @(Get-ChildItem -LiteralPath ($directory | Select-Object -Last 1) -Filter 'libstd-*.rlib' -ErrorAction SilentlyContinue)
    if ($libraries.Count -eq 0) { return $false }
    & $rustup run $toolchain cargo --version 2>$null | Out-Null
    return $LASTEXITCODE -eq 0
}

if (!(Test-SigningToolchain)) {
    $env:RUSTUP_DIST_SERVER = 'https://static.rust-lang.org'
    $env:RUSTUP_UPDATE_ROOT = 'https://static.rust-lang.org/rustup'
    & $rustup toolchain install $toolchain --profile minimal --target $target --no-self-update
    if ($LASTEXITCODE -ne 0 -or !(Test-SigningToolchain)) { throw 'Pinned signing toolchain is unavailable' }
}
Write-Output "Using installed signing toolchain $toolchain"
"RUSTUP_TOOLCHAIN=$toolchain" >> $env:GITHUB_ENV

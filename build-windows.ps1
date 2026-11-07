param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    $cargoRoot = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $flags = @('-C','target-feature=+crt-static',"--remap-path-prefix=$PSScriptRoot=.","--remap-path-prefix=$cargoRoot=cargo")
    $oldFlags = $env:CARGO_ENCODED_RUSTFLAGS
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
    cargo build --release --locked --target $Target
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
    New-Item -ItemType Directory -Force -Path 'Standalone' | Out-Null
    Copy-Item -LiteralPath "target\$Target\release\workflow-media.exe" -Destination 'Standalone\WorkFlow Media.exe' -Force
    Get-FileHash -Algorithm SHA256 -LiteralPath 'Standalone\WorkFlow Media.exe'
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $oldFlags
    Pop-Location
}

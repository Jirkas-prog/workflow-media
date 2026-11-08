param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    if ($Target -ne 'x86_64-pc-windows-msvc') { throw 'Bundled tools currently target Windows x64' }
    & (Join-Path $PSScriptRoot 'prepare-tools.ps1')
    $cargoRoot = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $flags = @('-C','target-feature=+crt-static',"--remap-path-prefix=$PSScriptRoot=.","--remap-path-prefix=$cargoRoot=cargo")
    $oldFlags = $env:CARGO_ENCODED_RUSTFLAGS
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
    cargo build --release --locked --target $Target --bin workflow-setup --features setup-stub
    if ($LASTEXITCODE -ne 0) { throw 'Setup downloader build failed' }
    $stub = "target\$Target\release\workflow-setup.exe"
    if ((Get-Item -LiteralPath $stub).Length -gt (5MB - 65536 - 56)) { throw 'Setup downloader exceeds the 5 MB attachment limit' }
    Copy-Item -LiteralPath $stub -Destination 'resources\setup-stub.exe' -Force
    cargo build --release --locked --target $Target --bin workflow-media --features setup-builder
    if ($LASTEXITCODE -ne 0) { throw 'Setup builder build failed' }
    New-Item -ItemType Directory -Force -Path 'SetupBuilder' | Out-Null
    $builderPath = Join-Path $PSScriptRoot 'SetupBuilder\WorkFlow Media Setup Builder.exe'
    $builder = Start-Process -FilePath (Join-Path $PSScriptRoot "target\$Target\release\workflow-media.exe") -ArgumentList @('--export-builder', '--output', ('"' + $builderPath + '"')) -Wait -PassThru -WindowStyle Hidden
    if ($builder.ExitCode -ne 0) { throw 'Setup builder export failed' }
    cargo build --release --locked --target $Target --bin workflow-media
    if ($LASTEXITCODE -ne 0) { throw 'Application build failed' }
    New-Item -ItemType Directory -Force -Path 'Standalone' | Out-Null
    Copy-Item -LiteralPath "target\$Target\release\workflow-media.exe" -Destination 'Standalone\WorkFlow Media.exe' -Force
    Get-FileHash -Algorithm SHA256 -LiteralPath 'Standalone\WorkFlow Media.exe'
    Get-FileHash -Algorithm SHA256 -LiteralPath $builderPath
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $oldFlags
    Pop-Location
}

param()
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$root = $PSScriptRoot
$cache = Join-Path $root '.build/tool-downloads'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
$lock = Get-Content -LiteralPath (Join-Path $root 'resources/tools-lock.json') -Raw | ConvertFrom-Json
foreach ($name in @('yt-dlp','deno','ffmpeg')) {
    $item = $lock.$name
    $path = Join-Path $cache ([IO.Path]::GetFileName($item.url))
    if (!(Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $item.sha256) {
        Write-Host "Downloading $name $($item.version)"
        Invoke-WebRequest -Uri $item.url -OutFile "$path.download"
        if ((Get-FileHash -LiteralPath "$path.download" -Algorithm SHA256).Hash -ne $item.sha256) { throw "Checksum mismatch: $name" }
        Move-Item -LiteralPath "$path.download" -Destination $path -Force
    }
}
$stage = Join-Path $cache ('bundle-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item -LiteralPath (Join-Path $cache 'yt-dlp.exe') -Destination $stage
foreach ($name in @('deno','ffmpeg')) {
    $archive = [IO.Compression.ZipFile]::OpenRead((Join-Path $cache ([IO.Path]::GetFileName($lock.$name.url))))
    try {
        foreach ($entry in $archive.Entries) {
            # Flatten only known executables and shared libraries. Preserve the upstream licence texts.
            $leaf = $entry.Name
            if (($name -eq 'deno' -and $leaf -eq 'deno.exe') -or ($name -eq 'ffmpeg' -and ($entry.FullName -match '/bin/(ffmpeg.exe|ffprobe.exe|[^/]+\.dll)$'))) {
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $stage $leaf), $true)
            } elseif ($name -eq 'ffmpeg' -and $entry.FullName -match '/(LICENSE|COPYING)[^/]*$') {
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $stage "ffmpeg-$leaf"), $true)
            }
        }
    } finally { $archive.Dispose() }
}
Copy-Item -LiteralPath (Join-Path $root 'THIRD-PARTY.md') -Destination $stage
Get-ChildItem -LiteralPath (Join-Path $root 'resources/licenses') -File | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $stage }
foreach ($name in @('yt-dlp','deno')) {
    $tag = if ($name -eq 'deno') { 'v' + $lock.$name.version } else { $lock.$name.version }
    $repo = if ($name -eq 'deno') { 'denoland/deno' } else { 'yt-dlp/yt-dlp' }
    $license = if ($name -eq 'deno') { 'LICENSE.md' } else { 'LICENSE' }
    Invoke-WebRequest -Uri "https://raw.githubusercontent.com/$repo/$tag/$license" -OutFile (Join-Path $stage "$name-LICENSE.txt")
}
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$($lock.'yt-dlp'.version)/THIRD_PARTY_LICENSES.txt" -OutFile (Join-Path $stage 'yt-dlp-THIRD-PARTY.txt')
$manifest = @{}
Get-ChildItem -LiteralPath $stage -File | ForEach-Object { $manifest[$_.Name] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
$utf8 = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText((Join-Path $stage 'manifest.json'), ($manifest | ConvertTo-Json), $utf8)
$bundle = Join-Path $root 'resources/tools.zip'
$pending = "$bundle.pending"
if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending }
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $pending, [IO.Compression.CompressionLevel]::Optimal, $false)
Move-Item -LiteralPath $pending -Destination $bundle -Force
Write-Host "Verified tools bundled: $bundle"

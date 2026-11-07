# Third-party software

WorkFlow Media's own Rust source is MIT-licensed. Third-party components retain their own licences; MIT does not relicense them. Rust dependency notices are in [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt), embedded in the app and accessible from Settings. Inter and Noto Sans Symbols 2 fonts use SIL OFL 1.1; their notices are beside the fonts.

## Windows media tools

The Windows EXE bundles separate, unmodified command-line programs. Rust invokes them as child processes; they are not linked into the application. `prepare-tools.ps1` downloads the pinned assets and checks the SHA-256 values in [resources/tools-lock.json](resources/tools-lock.json). On first use, the app extracts the tools and their notices into its versioned user-data `tools` directory.

| Component | Version | Licence and source |
| --- | --- | --- |
| yt-dlp | 2026.10.26 | [Unlicense and third-party notices](https://github.com/yt-dlp/yt-dlp/tree/2026.10.26). The official Windows executable contains Python and additional components with their own licences; both LICENSE and THIRD_PARTY_LICENSES.txt are included in the tool bundle. |
| Deno | 2.9.8 | [MIT and source](https://github.com/denoland/deno/tree/v2.9.8); includes Rusty V8 150.4.0 and [V8](https://github.com/denoland/v8/tree/ac1e23989121713ca642f6650b34deff7b686896). Their MIT/BSD notices are included. |
| FFmpeg / FFprobe | n8.1.3-34-ga35157abb9, LGPL shared build | [LGPL 3.0](https://github.com/FFmpeg/FFmpeg/blob/a35157abb9/COPYING.LGPLv3), with [GPL 3.0 terms incorporated by LGPL](resources/licenses/GPL-3.0.txt). Built by [FFmpeg-Builds](https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/tag/autobuild-2026-11-07-12-47). The FFmpeg executables and shared libraries remain separate files in the tool cache. |

FFmpeg's exact [upstream source](https://github.com/FFmpeg/FFmpeg/tree/a35157abb9) and the pinned [build recipes, patches and dependency source locations](https://github.com/FFmpeg-Builds/FFmpeg-Builds/tree/c63bb308a1de656b425c132a8102f8fd5389afc5) are available publicly. The release includes a source/recipe archive for convenience. Dependencies enabled in that LGPL build retain their licences; the build recipes identify their upstream sources. The build configuration is obtainable with `ffmpeg -buildconf`. The FFmpeg shared build excludes GPL-only codec libraries such as x264 and x265.

You may modify or replace the LGPL components and rebuild the tool bundle and application using the supplied sources and scripts. Reverse engineering for debugging modifications to LGPL components is permitted. The cache's integrity hashes protect ordinary downloads; to use modified tools, regenerate `manifest.json`/`resources/tools.zip` and rebuild your own application. Source changes must retain applicable notices and licences.

Linux uses the installed yt-dlp, Deno and FFmpeg programs and the system's GTK/AppIndicator/keyring libraries. Their packages have their own licence and source distribution arrangements.

## Rebuilding notices

Run Cargo metadata for the Windows and Linux targets and pass those local JSON files to `collect-licenses.py`. It emits upstream notices and public crate URLs without local developer paths. Tool licences are collected separately by `prepare-tools.ps1`. The old ytdown/Boa, containr, Symphonia and vendored shine-rs media pipeline is no longer used.

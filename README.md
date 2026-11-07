# WorkFlow Media

A Rust desktop downloader with a dark interface, a download queue and a separate media library. English and Czech are available in the setup guide and Settings.

[![Download for Windows](https://img.shields.io/badge/DOWNLOAD_FOR_WINDOWS-PORTABLE_EXE-c4fa67?style=for-the-badge&logo=windows&logoColor=152731)](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Windows-x64.exe)

[Česky](README.cs.md) · [Linux build guide](BUILD-LINUX.md) · [Build and release](BUILD.md) · [Licenses](THIRD-PARTY.md)

## Features

- Native Rust UI using egui/eframe, including menus, scrollbars, vector navigation icons and a virtualized table.
- MP4/MKV video and MP3 audio, best available quality by default, with individual service presets.
- YouTube extraction, FastShare account/search/downloads, public Facebook and Vimeo media, and the Instagram/TikTok extractors provided by ytdown.
- HTTP downloads, container assembly and audio encoding run inside the application. No Python, .NET, WebView, yt-dlp, FFmpeg or Deno installation is required.
- Clipboard monitoring for selected services, autostart and a tray menu. Minimize keeps the taskbar button; closing hides to the tray.
- One running instance per user profile: opening the EXE again restores the existing window.
- Header menus for sorting/filtering, optional columns, file information, copy/move without overwriting, and confirmed recycle-bin deletion.
- Signed GitHub release updates, checked manually or automatically when enabled.

## Windows

Download the portable x64 EXE into a writable folder and open it. The initial guide asks about background startup, clipboard monitoring, update preferences, language, folder, format and quality. Windows may show an administrator prompt for the initial setup check; normal operation runs without elevation.

The program is one executable. It creates `WorkFlow Media.preset.json` beside itself and stores the queue/library separately in the user's application-data directory. Completed downloads stay in the selected destination. Interrupted media streams are kept in `.workflow-partials` inside that destination for resume.

FastShare username, password and session are encrypted with Windows DPAPI. They are not placed in the portable preset. **Sign out and forget** removes the stored credentials. On Linux the application uses Secret Service; without an available, unlocked keyring it reports a storage error instead of saving the password in plain text.

## Service compatibility

Websites change their APIs and can require authentication, cookies, region access or CAPTCHA. This release's live media validation covers YouTube MP4/MKV/MP3. Facebook, Instagram, TikTok and Vimeo support depends on the URLs/extractors and is not a guarantee that every post, playlist or private video can be downloaded. Unsupported or encrypted streams report an error; DRM is not bypassed.

MP4 prefers compatible MP4 source streams. MP3 uses supported decodable audio (including AAC); Opus-only audio is currently unsupported for MP3 conversion. Unknown source resolution cannot be reliably limited. The app does not upscale video. Linux builds use desktop GTK/AppIndicator libraries for the tray; clipboard and tray behavior also depends on the desktop environment, particularly on Wayland.

## Source

Application code, build scripts, the locked dependency versions and release-signing instructions are included. The old Windows Forms implementation is not required to build or run this project. See [BUILD.md](BUILD.md).

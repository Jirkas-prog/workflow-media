# WorkFlow Media

A Rust desktop downloader with a dark interface, a download queue and a separate media library. English and Czech are available in the setup guide and Settings.

[![Download for Windows](https://img.shields.io/badge/DOWNLOAD_FOR_WINDOWS-PORTABLE_EXE-c4fa67?style=for-the-badge&logo=windows&logoColor=152731)](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Windows-x64.exe)

[![Download Setup Builder](https://img.shields.io/badge/SETUP_BUILDER-WINDOWS_EXE-86c8db?style=for-the-badge&logo=windows&logoColor=152731)](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Setup-Builder-Windows-x64.exe)

[Česky](README.cs.md) · [Setup Builder guide](SETUP-BUILDER.md) · [Linux build guide](BUILD-LINUX.md) · [Build and release](BUILD.md) · [Licenses](THIRD-PARTY.md)

## Features

- Native Rust UI using egui/eframe, including menus, scrollbars, vector navigation icons and a virtualized table.
- MP4/MKV video and MP3 audio, best available quality by default, with individual service presets.
- YouTube, Facebook, Instagram, TikTok, Vimeo and other sites supported by yt-dlp; separate Rust FastShare account/search/downloads.
- yt-dlp handles media extraction and downloads, Deno handles YouTube JavaScript, and FFmpeg joins/converts media. Windows bundles these tools inside one EXE; no manual Python or media-tool installation is required.
- Optional cookies from Chrome, Brave, Firefox, Edge, Chromium, Opera or Vivaldi, selected in Settings or setup. A Netscape cookies.txt export remains available.
- Clipboard monitoring for selected services, autostart and a tray menu. Minimize keeps the taskbar button; closing hides to the tray.
- One running instance per user profile: opening the EXE again restores the existing window.
- Header menus for sorting/filtering, optional columns, file information, copy/move without overwriting, and confirmed recycle-bin deletion.
- Signed GitHub release updates, checked manually or automatically when enabled.
- Up to 16 parallel downloads and a setup guide that can be reopened from Settings.
- A separate Windows Setup Builder that creates one EXE to download, verify, install and configure the app for someone else.

## Windows

Download the portable x64 EXE into a writable folder and open it. The initial guide asks about background startup, clipboard monitoring, update preferences, language, folder, format, quality and browser cookies. Windows may show an administrator prompt for the initial setup check; normal operation runs without elevation.

The Windows download is one executable. On the first media operation it extracts the bundled tools into a versioned `tools` folder in its user-data directory; this takes a few seconds. Later operations reuse the verified cache. The main app and Setup Builder are larger because they contain these tools. It creates `WorkFlow Media.preset.json` beside itself and stores the queue/library separately in the user's application-data directory. Completed downloads stay in the selected destination. Interrupted media streams are kept in `.workflow-partials` inside that destination for resume.

FastShare username, password and session are encrypted with Windows DPAPI. They are not placed in the portable preset. **Sign out and forget** removes the stored credentials. On Linux the application uses Secret Service; without an available, unlocked keyring it reports a storage error instead of saving the password in plain text.

## Prepare an installation for someone else

Open **WorkFlow Media Setup Builder**, go through its five steps and choose **Create installer EXE**. Share the resulting file. The recipient opens it, approves the administrator check, and the installer downloads the latest signed release, writes the prepared preset and launches the application. Internet access is required during installation.

By default, the app goes into **Documents → WorkFlow Media** and its shortcut onto **Desktop**. Both locations are configurable and resolved from the recipient's Windows special folders, including OneDrive redirection. Passwords, cookies and your local files are excluded. See the [full guide](SETUP-BUILDER.md).

## Service compatibility

Websites change their APIs and can require authentication, cookies, region access or CAPTCHA. This release's live media validation covers YouTube MP4/MKV/MP3. Facebook, Instagram, TikTok and Vimeo support depends on the URLs/extractors and is not a guarantee that every post, playlist or private video can be downloaded. Unsupported or encrypted streams report an error; DRM is not bypassed.

MP4/MKV use the best available streams within the selected resolution cap and remux without upscaling. MP3 conversion uses FFmpeg, including Opus input. Unknown source resolution cannot be reliably limited. Linux builds use desktop GTK/AppIndicator libraries for the tray; clipboard and tray behavior also depends on the desktop environment, particularly on Wayland.

## Browser cookies

Enable **Settings → Cookies for signed-in services → Automatically read cookies from a browser** and select the browser where you are signed in. This preference is also available in both setup guides. yt-dlp chooses that browser's default/recent profile on the current computer and reads it at download time. It is not exported or copied to the portable preset. The app does not ask for the browser password or elevate the downloader.

If the cookie database is locked, close the browser, including background processes. Modern Chromium-based browser encryption can prevent access even when the browser is closed. The app reports this; use Firefox or your own Netscape-format cookies.txt export instead. Selecting a browser does not guarantee access to private, region-restricted, protected or removed content. See the [yt-dlp cookie FAQ](https://github.com/yt-dlp/yt-dlp/wiki/FAQ#how-do-i-pass-cookies-to-yt-dlp).

## Source

Application code, build scripts, the locked dependency versions and release-signing instructions are included. The old Windows Forms implementation is not required to build or run this project. See [BUILD.md](BUILD.md).

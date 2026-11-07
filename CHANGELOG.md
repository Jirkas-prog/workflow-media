# Changelog

## 0.7.0

- Replaced the experimental native media extractors with yt-dlp for extraction, downloading, resume and post-processing. The UI, queue, FastShare, installer and updater remain Rust.
- Windows x64 ships one EXE containing pinned, SHA-256-verified yt-dlp, Deno and FFmpeg tools; no manual runtime installation is needed. Tools are extracted to a versioned user cache on first use.
- Added browser-cookie selection to Settings, the four-page setup guide and the five-page Setup Builder: Chrome, Brave, Firefox, Edge, Chromium, Opera and Vivaldi.
- Generated installers transfer only the selected browser, never the author's cookies or browser profile. Cookies are read from the recipient's own profile when downloading.
- Added process-tree cancellation, preserved partial downloads, atomic output publication without overwriting files, and English/Czech guidance for browser locks and encryption errors.
- Updated Linux dependency instructions and third-party software notices.

## 0.6.0

- Added a separate native Windows Setup Builder that creates one installer EXE containing portable recipient preferences.
- Added configurable installation and shortcut destinations, defaulting to the recipient's Documents and Desktop and respecting Windows folder redirection.
- Installers download and verify the latest signed GitHub release, apply the prepared preset and startup preference, create the selected shortcut and launch the app.
- Added installation rollback and isolated integration-test support; personal credentials, cookies and history are excluded from exported profiles.
- Made Run setup again directly accessible in Settings, with cancellation restoring the previous choices.
- Fixed responsive settings layout: long paths wrap inside their fields and grow vertically without squeezing labels or pushing buttons outside the window.
- Included English/Czech builder documentation and Windows/Linux build instructions.

## 0.5.3

- Increased the configurable parallel-download limit to 16, including validation and saved settings.
- Refined text fields, buttons and dropdowns with clearer borders, consistent padding and distinct hover, focus and pressed states.
- Preserved the existing page layout and dark WorkFlow appearance.

## 0.5.2

- Kept FastShare account controls exclusively in Settings, giving search results more room.
- Made single-instance activation reliable during startup: reopening the application restores the existing window, including from the tray.

## 0.5.1

- Fixed restoring the Windows application after hiding it to the notification area.
- Kept tray commands, update handling and second-launch activation responsive while the window is hidden or minimized.

## 0.5.0

- Reimplemented the desktop application in Rust with egui/eframe. Removed the dependency on the legacy Windows Forms UI and external media executables.
- Added native video extraction, resumable HTTP transfers, MP4/MKV assembly (including fragmented MP4) and MP3 encoding.
- Added vector navigation icons, a unified statistics strip and a clearly separated table with flat interactive headers.
- Preserved queue/library separation, filtering/sorting, hidden columns, tray behavior, clipboard service selection and the portable setup preset.
- Added English/Czech controls and protected, persistent FastShare credentials including the password.
- Separated FastShare search and account controls into clearer cards with larger, aligned inputs.
- Added signed release updates and Linux build instructions.

Service-specific compatibility and desktop limitations are documented in README.md.

# Changelog

## 0.5.0

- Reimplemented the desktop application in Rust with egui/eframe. Removed the dependency on the legacy Windows Forms UI and external media executables.
- Added native video extraction, resumable HTTP transfers, MP4/MKV assembly (including fragmented MP4) and MP3 encoding.
- Added vector navigation icons, a unified statistics strip and a clearly separated table with flat interactive headers.
- Preserved queue/library separation, filtering/sorting, hidden columns, tray behavior, clipboard service selection and the portable setup preset.
- Added English/Czech controls and protected, persistent FastShare credentials including the password.
- Added signed release updates and Linux build instructions.

Service-specific compatibility and desktop limitations are documented in README.md.

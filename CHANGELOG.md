# Changelog

## 0.8.6

- Added complete built-in Dark and Light palettes, independent of the operating system. Fixed the default light system style replacing dark controls while leaving a dark background. The theme is selectable and saved in Settings, setup and generated recipient installers.
- Added a small Windows update helper: locate the existing app, save and stop transfers, verify the signed GitHub release, replace/restart, then resume interrupted downloads while preserving manual pauses and settings. Older 0.8.x releases have a bounded tray-dialog compatibility adapter; new releases use process-lifetime-bound shutdown requests.
- Added theme persistence, contrast, system-theme isolation, setup interaction and queue-resume regression coverage.

## 0.8.5

- Generated recipient installers now contain a separate small Rust downloader and the portable preset, instead of copying the entire application and its media tools. The downloader is approximately 4 MB; generation and the Windows build enforce a 5 MiB ceiling including the preset.
- The separate Setup Builder no longer bundles yt-dlp, Deno or FFmpeg. The main single-file application still includes all three tools.
- Kept signed GitHub downloads, recipient folder selection, prepared settings, startup/shortcut behavior and installation rollback. Added size and packaging regression coverage.

## 0.8.4

- Fixed the name filter closing as soon as its text field was clicked. Column menus now stay open for input; typing, pasting, editing and table refreshes preserve focus. Escape, outside clicks, the header toggle and completed menu actions still close the menu.
- Added an input-event regression test covering the download table and library in English and Czech, including repeated clicks, live filtering to no matches, paste, cursor movement, backspace and dismissal.

## 0.8.3

- Replaced download progress bars in the file table with plain percentage text, matching completed rows.

## 0.8.2

- Every media download and metadata lookup now starts without cookies. Browser discovery, decryption, cache locking and refresh are delayed until an access/sign-in failure; public downloads no longer depend on accessible browser profiles.
- Added persistent account priorities in all-profile mode, numbered rows and enabled/disabled move arrows. Rescanning/refreshing preserves the order and selection; newly discovered profiles follow existing accounts. The same controls are available in Settings and setup.
- Fixed large YouTube metadata responses by requesting only the title and compact format fields. Full responses containing signed segment URLs could exceed the process reader frame size and cause missing-metadata errors.
- Kept anonymous, browser-account and manual-cookie partial transfers separate. Added anonymous-first real HTTP coverage, no-cookie-I/O tests and priority persistence tests.

## 0.8.1

- Fixed account fallback misclassifying local file/tool failures or digits in video IDs as authentication or rate-limit errors.
- Preserved original Netscape cookie exports during metadata inspection as well as downloads; temporary copies are removed on success and failure.
- Cancelling and reopening setup now discards pending profile results. Saving waits for profile loading to finish, and application shutdown cancels and joins the cookie worker before exiting.
- Corrected cookie-refresh timing after the system clock moves backwards.
- Added a real local HTTP integration test that denies seven accounts, serves the video to the eighth, verifies identical output bytes and checks that Set-Cookie responses do not modify the original export.

## 0.8.0

- Added separate browser-cookie identities: one default profile, selected profiles in a configurable order, or all profiles from selected browsers. Settings and both setup guides include the new controls.
- Downloads try the next profile after access/sign-in errors and stop at the first success. Unreadable profiles are skipped; network, disk and rate-limit errors do not trigger account cycling. Partial transfers are isolated by profile.
- Added automatic cookie refresh before downloads, manual refresh, encrypted snapshots and a forget action. A failed automatic refresh preserves the previous snapshot. Expired browser logins still require signing in through the browser.
- Setup Builder exports browser groups and refresh preferences without personal cookies, account names or profile paths. Recipient profiles are discovered locally.
- Added eight-profile synthetic integration coverage, encrypted-cache/refresh tests, portable-installer tests and responsive EN/CZ form checks.

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

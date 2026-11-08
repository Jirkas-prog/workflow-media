# Validation and test coverage

The 0.8.4 name-filter regression test replays pointer and keyboard events through the actual file table in both English and Czech, for downloads and the library. It reproduced the original click-to-close failure and now checks retained focus during typing, paste, editing and table refreshes, plus Escape, outside/header clicks, clear and sort actions. This adds one ordinary test to the counts below.

The 0.8.2 audit runs 45 ordinary tests on Windows and 43 on Ubuntu 24.04, plus three opt-in Windows integrations using the real media tools. Commands and fixture preparation are in [BUILD.md](BUILD.md#synthetic-multi-profile-test-windows).

| Area | Automated coverage |
| --- | --- |
| Cookies | Six Chrome and two Brave profiles; discovery, saved priority after refresh/restart (selected and all-profile modes), separate identities, encrypted snapshots, refresh, clock rollback, disabled/unreadable profiles and temporary-file cleanup |
| Authentication fallback | Real HTTP server checks an anonymous attempt before denying seven identities and serving an MP4 to the eighth; public media succeeds without preparing browser cookies; output bytes match the fixture; rate-limit, disk and tool errors do not trigger account cycling |
| Anonymous first | Public lookup/download succeeds with invalid or absent cookie configuration; cookie file/browser preparation is never reached; non-access errors and cancellation do not read accounts |
| Original cookie exports | Metadata inspection and downloads use temporary copies; server Set-Cookie responses leave the original file unchanged |
| Downloads | Real cancellation/resume with a nonzero HTTP Range; unknown-size progress; format arguments; completed files do not overwrite existing names |
| Queue and library | Independent removal of records; copy/move preserves bytes and rejects overwrites; display sorting leaves queue order intact |
| Settings and setup | EN/CZ forms at 480/720/1100 widths; long paths wrap; eight profile rows; cancelled setup rejects late results; shutdown waits for cookie cleanup |
| Installer | Portable recipient folders, completed presets, rollback, native Windows shortcut creation, overlay integrity, no author profiles/cookies in exported choices |
| Updates and tools | Signed manifest fields, verified tool extraction, tampered-cache repair and path boundaries |
| Other | Clipboard URL filtering, duplicate URLs, 1–16 download limit, legacy settings, protected FastShare credential storage and scoped redirects |

The affected YouTube link previously returned an 11 MB metadata document that exceeded the process reader frame size. The fixed release requests only title and compact format fields and successfully reads all 195 available formats.

Release verification also checks the actual single-file EXE, builder overlay, Ed25519 signature, published asset hashes and an isolated installation from GitHub. The isolated installer test deliberately skips UAC, autostart registration, shortcut placement and launch of a personal installation; native shortcut behavior is covered separately.

This is not a claim that every website or private account works. Personal Instagram/Facebook/FastShare accounts are not used by the automated tests. Chromium encryption may prevent cookie extraction. Linux cookie storage needs an unlocked Secret Service keyring; Windows DPAPI integration is exercised on Windows. Headless layout checks do not replace manual visual/interaction checks: the native UI automation attempt during the preceding audit could not complete because application approval timed out. No new native visual check is claimed for this release.

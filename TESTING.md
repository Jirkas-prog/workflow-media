# Validation and test coverage

The 0.8.1 audit runs 42 ordinary tests on Windows and 40 on Ubuntu 24.04, plus three opt-in Windows integrations using the real media tools. Commands and fixture preparation are in [BUILD.md](BUILD.md#synthetic-multi-profile-test-windows).

| Area | Automated coverage |
| --- | --- |
| Cookies | Six Chrome and two Brave profiles; discovery, selection/order, separate identities, encrypted snapshots, refresh, clock rollback, disabled/unreadable profiles and temporary-file cleanup |
| Authentication fallback | Real HTTP server denies seven identities and serves an MP4 to the eighth; output bytes match the fixture; rate-limit, disk and tool errors do not trigger account cycling |
| Original cookie exports | Metadata inspection and downloads use temporary copies; server Set-Cookie responses leave the original file unchanged |
| Downloads | Real cancellation/resume with a nonzero HTTP Range; unknown-size progress; format arguments; completed files do not overwrite existing names |
| Queue and library | Independent removal of records; copy/move preserves bytes and rejects overwrites; display sorting leaves queue order intact |
| Settings and setup | EN/CZ forms at 480/720/1100 widths; long paths wrap; eight profile rows; cancelled setup rejects late results; shutdown waits for cookie cleanup |
| Installer | Portable recipient folders, completed presets, rollback, native Windows shortcut creation, overlay integrity, no author profiles/cookies in exported choices |
| Updates and tools | Signed manifest fields, verified tool extraction, tampered-cache repair and path boundaries |
| Other | Clipboard URL filtering, duplicate URLs, 1–16 download limit, legacy settings, protected FastShare credential storage and scoped redirects |

Release verification also checks the actual single-file EXE, builder overlay, Ed25519 signature, published asset hashes and an isolated installation from GitHub. The isolated installer test deliberately skips UAC, autostart registration, shortcut placement and launch of a personal installation; native shortcut behavior is covered separately.

This is not a claim that every website or private account works. Personal Instagram/Facebook/FastShare accounts are not used by the automated tests. Chromium encryption may prevent cookie extraction. Linux cookie storage needs an unlocked Secret Service keyring; Windows DPAPI integration is exercised on Windows. Headless layout checks do not replace manual visual/interaction checks: the native UI automation attempt during this audit could not complete because application approval timed out.

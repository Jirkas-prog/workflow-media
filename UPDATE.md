# Immediate update helper (Windows x64)

[Download the small EXE](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Update-Windows-x64.exe).

Run it with an existing WorkFlow Media application running. It finds the registered instance (0.8.6+) or the older WorkFlow window, including a tray-hidden window. If no app is running it also checks beside the helper and the recipient's Documents/WorkFlow Media folder. It refuses ambiguous instances. Run as the same user and privilege level as the app; it does not elevate or access another user's installation.

The update saves/stops transfers before network activity, verifies the Ed25519-signed stable release and SHA-256, stages the replacement on the application volume, retains a `.previous` binary, and restarts with the same data and preset. It resumes only jobs interrupted by this operation, preserves manual/global pauses and never edits the preset. A newer installed version is not downgraded. An identical current version is simply restarted. A network/verification failure restarts the previous app and reports the failure. An inability to stop or lock the existing app aborts without replacing it. An unfinished operation retains a local recovery record for the next attempt.

Version 0.8.6+ uses a per-process nonce and the existing instance lock. Versions 0.8.0–0.8.5 need a narrowly scoped compatibility adapter: it invokes their actual tray Quit item, then confirms the existing modal via window-specific key messages. No process is forcibly killed. The old dialog may appear briefly. Unresponsive/elevated legacy windows can prevent this adapter from completing; the helper reports the failure. The old Windows Forms application is not an update target.

Advanced/custom installations and isolated tests can specify paths explicitly:

```powershell
& './WorkFlow Media Update.exe' --application 'C:\Apps\WorkFlow Media\WorkFlow Media.exe' --data 'C:\AppData\WorkFlow Media' --preset 'C:\Apps\WorkFlow Media\WorkFlow Media.preset.json'
```

All paths must be absolute. `--headless` suppresses the progress/error windows and returns a nonzero exit code on failure; it does not change the verification or shutdown requirements. No passwords, cookies or preset need to be included with the helper.

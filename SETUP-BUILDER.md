# Windows Setup Builder

[Česky](SETUP-BUILDER.cs.md) · [Download Setup Builder](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Setup-Builder-Windows-x64.exe)

Setup Builder is a separate portable Windows x64 EXE. It creates a single installer EXE containing the recipient's preferences. Neither person needs Rust or another runtime installed. The builder works offline; the generated installer requires Internet access to GitHub.

The **generated recipient installer is approximately 4 MB** (under 5 MiB including settings). It contains a small Rust downloader, not the full application or its media tools. The separate graphical Builder is larger than the file it produces. Use Builder 0.8.5 or newer to regenerate older, oversized installers; existing files do not shrink automatically.

## Prepare the file

1. Open **WorkFlow Media Setup Builder.exe**, or choose **Settings → Open setup builder** in the main app.
2. On the first page select language, download subfolder, parallel-download limit, startup, tray behavior and automatic updates for the recipient.
3. On the second page choose automatic downloads from copied links: off, selected services, or all supported services.
4. On the third page choose default format and quality, with optional service presets.
5. On the fourth page optionally enable browser cookies. Choose one default profile or **Load all browser cookies**, select the recipient's browsers, and choose whether to refresh cookies automatically. Profiles are discovered on the recipient's computer; no actual cookies, account names or profile paths are included.
6. On the final page review the summary and choose the installation and shortcut locations. Click **Create installer EXE** and save the file.

Creating an installer does not change the author's running application or register startup on the author's computer. Reopen the builder to prepare another configuration.

Share the generated EXE through a trusted delivery method. Some email services block executable attachments or enforce size limits; a download link may be more practical. The builder does not send anything automatically.

## Recipient locations

The defaults are **Documents / WorkFlow Media** for the application and **Desktop** for its shortcut. The author can select:

| Item | Available destinations |
| --- | --- |
| Application | Documents, Desktop, Downloads, local user applications |
| Application subfolder | A single folder name, default `WorkFlow Media` |
| Shortcut | Desktop, Documents, Downloads, Start menu, or no shortcut |
| Downloaded media | A named subfolder of the recipient's Downloads directory |

The profile stores folder identifiers rather than the author's absolute paths. Windows resolves Documents, Desktop and Downloads on the recipient's machine through its special-folder API, including renamed, localized or redirected folders and OneDrive. Local user applications means `%LOCALAPPDATA%/Programs`. There is no hard-coded username or assumption that Documents/Desktop are on the C drive. A missing special folder produces an error instead of silently using an unrelated location.

## Installation

1. The recipient opens the generated EXE and approves its Windows administrator prompt. Cancelling stops installation.
2. The installer obtains the current stable release from `Jirkas-prog/workflow-media`, verifies the Ed25519-signed update manifest and checks the downloaded EXE's SHA-256.
3. It writes `WorkFlow Media.exe` and `WorkFlow Media.preset.json` into the selected application folder. Setup is marked complete, so the app opens directly.
4. It creates the chosen shortcut, applies the selected sign-in startup setting and opens the configured app.

The administrator prompt is a short authorization check. Installation, shortcuts and startup belong to the original recipient's account even if another administrator supplies credentials. The downloader runs with normal user permissions.

With startup enabled, future sign-ins start the app in the tray. Minimize keeps its taskbar button; closing hides it to the tray when enabled. **Settings → Run setup again** reopens the normal guide.

If the app is already running, the installer asks the recipient to quit it through the tray menu and retry. It does not terminate downloads. Installing again into the same folder updates setup choices but preserves the local cookie-file selection, hidden columns, separate queue/library and encrypted FastShare credentials. Failed application/preset writes or startup registration restore the previous app and preset. Shortcut or launch failures are reported after installation. Choosing a different application folder creates a new copy and leaves the old files in place.

## Included configuration

Language, download subfolder, parallel downloads, startup, close-to-tray, start-on-add, clipboard mode/services, automatic updates, browser-cookie mode/browser selection/automatic refresh, default format/quality, service presets, application location/subfolder and shortcut location.

The recipient can refine the discovered accounts and their order in Settings. The first attempt always uses no cookies. Each profile is then tried separately on access errors, stopping at the first success. Account priorities also apply to all-profile mode and survive refresh. Automatic refresh reads the browser's current cookies only when authentication is needed, at most once per minute per profile; it does not renew an expired login. If refresh fails, a previous encrypted snapshot is retained. Browser locks and encryption can prevent extraction; use an accessible profile or a personal cookie export. See [Browser cookies](README.md#browser-cookies).

Passwords, cookies, FastShare sessions, browser profiles, media, history and the author's absolute paths are excluded. The profile contains settings, not arbitrary commands or a custom download URL. It always uses the application's compiled GitHub release channel and public verification key.

Release-signature verification is separate from Windows Authenticode; the EXEs do not currently carry a commercial Windows publisher certificate.

## Build and development

`build-windows.ps1` first compiles `workflow-setup` with `setup-stub`, checks its size and embeds it from the ignored `resources/setup-stub.exe`. It then builds Setup Builder with `setup-builder` (without media tools) and finally the complete app. The results are `Standalone/WorkFlow Media.exe` and `SetupBuilder/WorkFlow Media Setup Builder.exe`. All three components are Rust. Generated installers embed a bounded, versioned JSON profile with an integrity checksum. No external scripts or compiler are required at runtime. A build without the downloader refuses installer generation instead of copying the full app.

The main application also builds on Linux; see [BUILD-LINUX.md](BUILD-LINUX.md). The installer generator targets Windows x64 and must be built and run on Windows.

Headless developer commands:

```powershell
./build-windows.ps1
& './SetupBuilder/WorkFlow Media Setup Builder.exe' --make-installer --profile profile.json --output setup.exe
./setup.exe --test-install-to C:\Temp\workflow-install-test
```

The profile schema is `installer::Profile` in `src/installer.rs`. The last command downloads and verifies a real release and writes the app/preset under the explicit test directory. This isolated test mode skips UAC, startup registration, shortcuts and launch; ordinary double-click installation performs those integration steps.

## Application theme

Choose **Dark** or **Light** in the application settings step. Both palettes are embedded in the app and ignore the recipient's OS theme. The choice is included in the generated portable preset; profiles created before 0.8.6 default to Dark.

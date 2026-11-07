# Build and release

Use Rust 1.95 or later; the development validation uses Rust 1.99.1. Cargo.lock pins the dependency graph. The first build requires Internet access for crates.

## Windows x64

Install Rust's MSVC toolchain and Visual Studio Build Tools with **Desktop development with C++** and the Windows SDK. In PowerShell, from this project:

```powershell
cargo test --locked
./build-windows.ps1
```

The results are `Standalone/WorkFlow Media.exe` and `SetupBuilder/WorkFlow Media Setup Builder.exe`. Close an older copy before overwriting its output file. See [SETUP-BUILDER.md](SETUP-BUILDER.md) for the installer workflow. The script statically links the C runtime, removes developer source paths and embeds fonts, icon, help and license notices. It does not package or launch any media helper executables. Windows system DLLs and a working graphics driver are still required.

For Linux see [BUILD-LINUX.md](BUILD-LINUX.md).

## Signed application updates

The update channel is `Jirkas-prog/workflow-media` on GitHub Releases. Keep the existing signing key private and backed up; never commit it. The public Ed25519 key is compiled from `resources/update-key.hex`. Only that public key belongs in Git.

For an independent fork, generate a cryptographically random 32-byte private key, change `REPOSITORY` in `src/updater.rs`, then export the matching public key **before** building the distributed executable:

```powershell
./target/debug/workflow-media.exe --sign-release --key PRIVATE_KEY_FILE --public-key resources/update-key.hex
./build-windows.ps1
```

For each release, increase the Cargo package version and rebuild. Copy the executable to `WorkFlow-Media-Windows-x64.exe`, then sign that exact asset:

```powershell
& './Standalone/WorkFlow Media.exe' --sign-release --key PRIVATE_KEY_FILE --release-version 0.6.0 --file WorkFlow-Media-Windows-x64.exe --output update-windows-x86_64.json
```

Publish both files in the stable `v0.6.0` GitHub release. Also publish the builder as `WorkFlow-Media-Setup-Builder-Windows-x64.exe` and include both EXE hashes in `SHA256SUMS.txt`. Never publish personal generated installers or profiles. The manifest signs version, target, immutable release URL and SHA-256. An altered manifest or binary is refused. A failed lookup reports an error; it is not presented as a successful update check. Never replace an already published version's binary with different bytes.

Automatic checks run once per day. Installation waits for an idle queue, pauses active work on manual restart, verifies the downloaded helper again, preserves a `.previous` copy and restarts with the same data/preset locations. Do not rotate the public key without an update-key migration plan: installed clients trust their embedded key.

## Developer commands

The same binary has headless diagnostics (`--inspect-url`, `--download-url`, `--convert-media`) and isolated UI preview flags (`--isolated --data ABSOLUTE_PATH --preset ABSOLUTE_PATH --no-uac`). A fresh `--setup-preview` displays the guide without saving its settings. Do not ship preview data, credentials, logs or signing keys.

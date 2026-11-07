# Build on Linux

These instructions build the main downloader, including its setup guide. The separate Setup Builder creates Windows x64 installers and runs on Windows only. Linux validation covers compilation and tests on Ubuntu 24.04 x86-64; other desktop integrations and architectures require testing on the intended system.

The Rust source is shared with Windows. The Linux target is x86-64 or aarch64 according to the machine/toolchain. GTK is used for the system tray, not for the application interface.

On Ubuntu 24.04 / Debian, install the compiler and desktop development libraries:

```bash
sudo apt update
sudo apt install build-essential pkg-config curl libgtk-3-dev \
  libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev
```

Install Rust using the official instructions at [rustup.rs](https://rustup.rs/). Rust 1.95+ is required. Then, inside the source directory:

```bash
cargo test --locked
bash build-linux.sh
./Standalone/workflow-media
```

The build produces one application binary. System GTK, AppIndicator, OpenGL/EGL and desktop libraries remain normal Linux runtime dependencies. On minimal systems also install a Secret Service provider (for example GNOME Keyring), a file chooser portal and a tray/AppIndicator extension supported by the desktop. Credentials are saved to the unlocked keyring. The app reports failure rather than falling back to plaintext.

The preset is written beside the binary; put it in a writable user-owned directory. The data directory follows XDG conventions. Autostart uses `~/.config/autostart`. Downloaded media stays in the configured folder.

On Wayland, background clipboard access and file-copy clipboard formats depend on compositor permissions and available clipboard utilities. Test these features on the intended desktop. A successful build does not imply every desktop integration was tested on every distribution.

If build memory is limited, use `CARGO_BUILD_JOBS=2 bash build-linux.sh`. For a distribution package, build on the oldest glibc baseline you intend to support.

# Third-party software

The application source is MIT-licensed. Its dependencies retain their own licenses. The full collected license texts and attribution notices are in [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt), also embedded in the executable and available from Settings.

- egui/eframe: MIT or Apache-2.0.
- ytdown: MIT or Apache-2.0; native media extraction, with Boa JavaScript execution inside Rust.
- containr and mp4-atom: see their notices; native container processing.
- Symphonia: MPL-2.0; native audio decoding. Exact package versions are in Cargo.lock; original corresponding source is available at `https://crates.io/crates/PACKAGE/VERSION` for each Symphonia package listed in the notices.
- shine-rs 0.1.5: LGPL-2.0. Its complete crate source is included in `vendor/shine-rs`; this project uses that source directly through Cargo's patch mechanism. You may modify or replace this library and rebuild/relink the application using the supplied complete application source and build scripts. Reverse engineering for debugging modifications to the LGPL component is permitted. The combined distribution remains subject to the applicable LGPL terms.
- Inter and Noto Sans Symbols 2 fonts: SIL Open Font License 1.1, with license files beside the font assets.

No third-party media executables are bundled. Linux system libraries have their own distribution licenses.

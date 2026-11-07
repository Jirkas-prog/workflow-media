# Kompilace pro Linux

Na Ubuntu 24.04 / Debianu připravte překladač a systémové knihovny:

```bash
sudo apt update
sudo apt install build-essential pkg-config curl libgtk-3-dev \
  libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev
```

Rust nainstalujte podle [rustup.rs](https://rustup.rs/), ve verzi 1.95 nebo vyšší. Ve složce zdrojů pak spusťte:

```bash
cargo test --locked
bash build-linux.sh
./Standalone/workflow-media
```

Výsledkem je jediný spustitelný soubor aplikace. Linuxové systémové knihovny GTK, AppIndicator a grafický ovladač zůstávají běžnými závislostmi systému. Aplikace nepotřebuje externí programy pro stahování či zpracování médií.

Program uložte do zapisovatelné složky: vedle něj vzniká předvolba. Data fronty a knihovny jsou v uživatelském adresáři podle XDG. Pro zapamatování hesla FastShare musí fungovat odemčená systémová klíčenka, například GNOME Keyring. Některá prostředí vyžadují rozšíření AppIndicator pro ikonu u hodin.

Sledování schránky na pozadí závisí na prostředí, zejména na Waylandu. Na cílovém desktopu je potřeba ověřit dostupnost schránky a tray. Při nedostatku paměti kompilujte s `CARGO_BUILD_JOBS=2 bash build-linux.sh`.

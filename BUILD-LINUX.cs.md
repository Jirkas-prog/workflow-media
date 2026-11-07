# Kompilace pro Linux

Tento návod kompiluje hlavní aplikaci včetně průvodce nastavením. Samostatný generátor instalace vytváří Windows x64 EXE a běží pouze ve Windows. Na Ubuntu 24.04 x86-64 byly ověřeny testy a kompilace; další prostředí a architektury vyžadují vlastní ověření.

Na Ubuntu 24.04 / Debianu připravte překladač a systémové knihovny:

```bash
sudo apt update
sudo apt install build-essential pkg-config curl libgtk-3-dev \
  libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev ffmpeg unzip
```

Rust nainstalujte podle [rustup.rs](https://rustup.rs/), ve verzi 1.95 nebo vyšší. Ve složce zdrojů pak spusťte:

```bash
cargo test --locked
bash build-linux.sh
./Standalone/workflow-media
```

Výsledkem je jediný spustitelný soubor aplikace. Linuxové systémové knihovny GTK, AppIndicator a grafický ovladač zůstávají běžnými závislostmi systému. Pro média Linux používá nástroje yt-dlp, Deno a FFmpeg dostupné v PATH.

Program uložte do zapisovatelné složky: vedle něj vzniká předvolba. Data fronty a knihovny jsou v uživatelském adresáři podle XDG. Pro zapamatování hesla FastShare musí fungovat odemčená systémová klíčenka, například GNOME Keyring. Některá prostředí vyžadují rozšíření AppIndicator pro ikonu u hodin.

Sledování schránky na pozadí závisí na prostředí, zejména na Waylandu. Na cílovém desktopu je potřeba ověřit dostupnost schránky a tray. Při nedostatku paměti kompilujte s `CARGO_BUILD_JOBS=2 bash build-linux.sh`.

## Nástroje pro média

Linux používá samostatně instalované `yt-dlp`, `deno` a `ffmpeg` z absolutních adresářů v `PATH`. Windows nástroje do Linuxu nekopírujte. Nainstalujte aktuální yt-dlp podle [oficiálního návodu](https://github.com/yt-dlp/yt-dlp#installation) (standalone Linux release nebo `pipx install "yt-dlp[default]"`) a Deno podle [oficiálního návodu](https://docs.deno.com/runtime/getting_started/installation/). FFmpeg je v balíčku výše. yt-dlp musí mít i podporu EJS pro YouTube; oficiální samostatný soubor ji obsahuje. Ověřte:

```bash
yt-dlp --version
deno --version
ffmpeg -version
```

Cesty přidané do shellového profilu nemusí být dostupné při spuštění z nabídky desktopu či autostartu. Zajistěte stejné PATH i pro grafickou relaci. Cookies z prohlížeče mohou vyžadovat odemčenou systémovou klíčenku. Sestavení bez těchto nástrojů funguje, ale stahování médií oznámí chybějící závislost.

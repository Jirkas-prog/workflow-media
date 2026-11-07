# WorkFlow Media

Aplikace běží v jediné instanci pro daný uživatelský profil. Opětovné otevření EXE zobrazí už běžící okno, i když je schované u hodin.

Samostatná aplikace v Rustu pro stahování a správu médií. Rozhraní je česky i anglicky.

[Stáhnout aplikaci pro Windows](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Windows-x64.exe) · [Kompilace Linuxu](BUILD-LINUX.cs.md)

Stačí jeden EXE soubor. Při prvním otevření vás průvodce provede nastavením. Vedle programu vznikne předvolba `WorkFlow Media.preset.json`; po jejím uložení se průvodce příště přeskočí. Soubor uložte do složky, do které můžete zapisovat.

- Výchozí kvalita je nejvyšší dostupná. Každá služba může mít vlastní kvalitu a formát.
- Sledování schránky má v průvodci předvolené vybrané služby: YouTube, Facebook a FastShare.
- Minimalizace nechá aplikaci na hlavním panelu. Křížek ji standardně schová k hodinám; nabídka pravého tlačítka umožňuje vložit či stáhnout odkaz.
- Filtry a řazení se otevírají ze záhlaví sloupců. Opětovné kliknutí nabídku zavře. Sloupce lze skrýt.
- Fronta a knihovna jsou oddělené. Vyčištění seznamu nemaže soubory. Samostatná akce s potvrzením přesune soubor do koše.
- FastShare přihlášení včetně hesla se zapamatuje šifrovaně pomocí Windows DPAPI nebo linuxové klíčenky. Tlačítko **Odhlásit a zapomenout** uložené údaje odstraní. Heslo není v předvolbě vedle EXE.
- Aktualizace z GitHub Releases se ověřují digitálním podpisem a kontrolním součtem. Lze je zapnout automaticky nebo vyvolat ručně.

Stahování, spojování obrazu a zvuku i tvorba MP3 probíhají uvnitř Rust aplikace. Nepotřebuje instalovat .NET, Python, yt-dlp, FFmpeg ani Deno.

Živě bylo ověřeno YouTube MP4, MKV a MP3. Dostupnost příspěvků na dalších službách závisí na jejich aktuálních pravidlech a přihlašování. MP3 zatím nepřevádí zvuk dostupný pouze v Opusu. DRM se neobchází. Více podrobností obsahuje [anglická dokumentace](README.md).

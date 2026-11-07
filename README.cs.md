# WorkFlow Media

Aplikace běží v jediné instanci pro daný uživatelský profil. Opětovné otevření EXE zobrazí už běžící okno, i když je schované u hodin.

Samostatná aplikace v Rustu pro stahování a správu médií. Rozhraní je česky i anglicky.

[Stáhnout aplikaci pro Windows](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Windows-x64.exe) · [Kompilace Linuxu](BUILD-LINUX.cs.md)

[Stáhnout generátor instalace pro Windows](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Setup-Builder-Windows-x64.exe) · [Návod generátoru](SETUP-BUILDER.cs.md)

Stačí jeden EXE soubor. Při prvním otevření vás průvodce provede nastavením. Vedle programu vznikne předvolba `WorkFlow Media.preset.json`; po jejím uložení se průvodce příště přeskočí. Soubor uložte do složky, do které můžete zapisovat.

- Výchozí kvalita je nejvyšší dostupná. Každá služba může mít vlastní kvalitu a formát.
- Sledování schránky má v průvodci předvolené vybrané služby: YouTube, Facebook a FastShare.
- Minimalizace nechá aplikaci na hlavním panelu. Křížek ji standardně schová k hodinám; nabídka pravého tlačítka umožňuje vložit či stáhnout odkaz.
- Filtry a řazení se otevírají ze záhlaví sloupců. Opětovné kliknutí nabídku zavře. Sloupce lze skrýt.
- Fronta a knihovna jsou oddělené. Vyčištění seznamu nemaže soubory. Samostatná akce s potvrzením přesune soubor do koše.
- FastShare přihlášení včetně hesla se zapamatuje šifrovaně pomocí Windows DPAPI nebo linuxové klíčenky. Tlačítko **Odhlásit a zapomenout** uložené údaje odstraní. Heslo není v předvolbě vedle EXE.
- Aktualizace z GitHub Releases se ověřují digitálním podpisem a kontrolním součtem. Lze je zapnout automaticky nebo vyvolat ručně.

Rozhraní, fronta, FastShare, instalátor i aktualizace zůstávají v Rustu. Média nově stahuje yt-dlp, YouTube JavaScript zpracovává Deno a spojování obrazu se zvukem a tvorbu MP3 zajišťuje FFmpeg. Windows EXE obsahuje všechny tyto nástroje. Při prvním použití je rozbalí do své datové složky; není potřeba je ručně instalovat. Kvůli přibaleným nástrojům je EXE větší.

Živě bylo ověřeno YouTube MP4, MKV a MP3. Dostupnost příspěvků na dalších službách závisí na jejich aktuálních pravidlech a přihlašování. MP3 nyní podporuje i převod zvuku z Opusu. DRM se neobchází. Více podrobností obsahuje [anglická dokumentace](README.md).

## Instalace pro dalšího člověka

Samostatný **WorkFlow Media Setup Builder** vás provede pěti kroky. Zvolíte nastavení příjemce a uložíte jeden instalační EXE. Příjemce jej otevře, potvrdí správce a instalátor stáhne aktuální podepsanou aplikaci z GitHubu, uloží připravené nastavení a spustí ji. Potřebuje internet.

Výchozí umístění je **Dokumenty → WorkFlow Media**, zástupce se vytvoří na **Ploše**. Obojí lze změnit. Použijí se skutečné systémové složky příjemce, nezávisle na jeho jménu nebo přesměrování do OneDrivu. Hesla, cookies, historie ani vaše osobní soubory se nepřenášejí.

Průvodce vlastní aplikace lze opět otevřít tlačítkem **Spustit průvodce znovu** nahoře v Nastavení. Souběžné stahování umožňuje 1–16 úloh. Dlouhé cesty se zalamují uvnitř polí a jejich výška roste podle obsahu.

## Cookies z prohlížeče

V **Nastavení → Cookies a profily prohlížečů** zapněte **Používat cookies z prohlížečů**. K dispozici jsou tři režimy:

- **Jeden výchozí profil:** yt-dlp použije výchozí nebo naposledy použitý profil vybraného prohlížeče.
- **Vybrané profily postupně:** tlačítkem **Vyhledat profily** najdete účty, zaškrtnete požadované a šipkami nastavíte jejich pořadí.
- **Vytáhnout všechny cookies:** vyberete Chrome, Brave, Firefox, Edge, Chromium, Operu nebo Vivaldi. Použijí se všechny nalezené profily těchto prohlížečů, včetně nově vytvořených. Stejnojmenné tlačítko je načte hned; jinak je načte první stahování.

Šest profilů Chrome a dva Brave tak zůstávají osmi samostatnými účty. Při odmítnutém přístupu nebo požadavku na přihlášení se zkusí další profil, při úspěchu zkoušení skončí. Nečitelný profil se přeskočí. Cookies ani rozpracované soubory různých účtů se neslučují. Při chybě sítě, disku, zrušení nebo limitu HTTP 429 se další účty nezkoušejí. Obsah musí být dostupný alespoň jednomu z účtů.

Volba **Automaticky aktualizovat cookies profilů** obnovuje uložené kopie před stahováním, nejvýše jednou za minutu pro každý profil. Při vypnutí se používá poslední kopie; první načtení proběhne vždy automaticky. Pokud automatická obnova selže, zůstává dostupná předchozí kopie. **Obnovit cookies nyní** vypíše případné chyby a nepřepíše funkční kopii neúspěšným exportem. Vypršené přihlášení obnovte v prohlížeči, aplikace se za vás znovu nepřihlašuje.

Kopie jsou chráněné pomocí Windows DPAPI nebo klíčenky Secret Service na Linuxu. Do předvolby se ukládají jen volby a místní cesty profilů, nikoli cookies. Dočasný soubor pro stahování obsahuje vždy jediný profil a po pokusu se odstraní. **Zapomenout uložené cookies** odstraní uložené kopie; další použití je načte znovu. Vypnutím cookies prohlížečů zastavíte další automatické načítání.

Stejné volby jsou v běžném průvodci. Generátor instalace umožňuje jeden výchozí profil nebo **Vytáhnout všechny cookies**, výběr prohlížečů a automatickou obnovu. Přenese pouze tyto volby. Profily najde až na počítači příjemce, vaše názvy účtů, cesty ani cookies do instalačního souboru nevloží.

Pokud prohlížeč zamyká databázi, zavřete i jeho procesy na pozadí. Moderní ochrana Chrome, Brave nebo Edge může načtení znemožnit; aplikace to oznámí. V takovém případě použijte Firefox nebo vlastní export cookies.txt ve formátu Netscape. Přístup k soukromému či omezenému obsahu vždy závisí na příslušné službě.

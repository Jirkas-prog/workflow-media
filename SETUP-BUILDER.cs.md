# Generátor instalace pro Windows

[English](SETUP-BUILDER.md) · [Stáhnout generátor](https://github.com/Jirkas-prog/workflow-media/releases/latest/download/WorkFlow-Media-Setup-Builder-Windows-x64.exe)

Samostatný **WorkFlow Media Setup Builder.exe** připraví jeden instalační EXE pro příjemce. Není potřeba Rust ani jiný runtime. Generátor funguje bez internetu; příjemce potřebuje internet ke stažení aktuální aplikace z GitHubu.

## Příprava

1. Otevřete generátor, případně v aplikaci zvolte **Nastavení → Otevřít generátor instalace**.
2. V prvním kroku vyberte jazyk příjemce, podsložku ve Stažených, souběžná stahování, spuštění po přihlášení, chování křížku a automatické aktualizace.
3. Ve druhém kroku nastavte automatické stahování ze schránky: vypnuté, vybrané služby, nebo všechny podporované služby.
4. Ve třetím kroku vyberte kvalitu a formát, případně předvolby jednotlivých služeb.
5. Ve čtvrtém kroku můžete zapnout cookies z prohlížeče a vybrat prohlížeč příjemce. Cookies ani cestu k vašemu profilu instalátor neobsahuje.
6. V posledním kroku zkontrolujte přehled a vyberte umístění aplikace i zástupce. Klikněte na **Vytvořit instalační EXE** a uložte soubor.

Výchozí instalace je **Dokumenty → WorkFlow Media**, zástupce je na **Ploše**. Aplikaci lze místo Dokumentů umístit na Plochu, do Stažených nebo mezi místní aplikace uživatele. Podsložku lze přejmenovat. Zástupce lze umístit na Plochu, do Dokumentů, Stažených, nabídky Start, nebo nevytvářet.

Cesty se zjistí až na počítači příjemce podle jeho systémových složek Windows. Fungují s jiným jménem uživatele, jiným jazykem Windows i přesměrováním Dokumentů/Plochy do OneDrivu nebo na jiný disk. Do instalačního souboru se nezapisuje vaše absolutní cesta.

Vzniklý EXE předejte příjemci. Některé e-mailové služby blokují spustitelné přílohy nebo omezují velikost; praktické může být poslat odkaz ke stažení. Generátor nic sám nikomu neposílá a nemění vaše nastavení ani automatické spouštění.

## U příjemce

Příjemce otevře EXE a potvrdí správce. Instalátor stáhne aktuální stabilní vydání z `Jirkas-prog/workflow-media`, ověří digitální podpis manifestu a kontrolní součet EXE. Potom uloží aplikaci a připravenou předvolbu do vybraného umístění, vytvoří zástupce na zvoleném místě, nastaví automatické spouštění podle vaší volby a otevře aplikaci.

Výzva správce slouží ke krátkému ověření oprávnění. Samotná instalace i automatické spouštění patří původnímu účtu příjemce, i když potvrzení provede jiný správce. Stahování běží s běžnými právy uživatele. Zrušení výzvy instalaci zastaví.

Soubory se stahují do zvolené podsložky ve Stažených příjemce. Při dalším přihlášení aplikace naběhne u hodin, pokud jste to zvolili. Minimalizace ji ponechá na hlavním panelu; křížek ji při zapnuté volbě schová k hodinám. Hotová předvolba přeskočí úvodní průvodce. Znovu jej lze otevřít přes **Nastavení → Spustit průvodce znovu**.

Pokud už aplikace běží, instalátor požádá o ukončení přes nabídku u hodin a nové spuštění instalace. Stahování násilně neukončí. Opakovaná instalace do stejné složky změní volby průvodce, ale zachová místní cestu cookies, skryté sloupce, samostatnou frontu/knihovnu a šifrované přihlášení FastShare. Při chybě zápisu aplikace, předvolby nebo registrace spouštění se obnoví předchozí aplikace i předvolba. Instalace do jiného umístění ponechá starou kopii na místě.

Přenášejí se jen volby nastavení. **Hesla, cookies, přihlašovací relace, historie, média ani vaše absolutní cesty se nepřibalují.** Ověření podpisu staženého vydání je nezávislé na certifikátu vydavatele Windows; EXE zatím nemá komerční podpis Authenticode.

## Kompilace

`build-windows.ps1` vytvoří aplikaci do `Standalone` a samostatný generátor do `SetupBuilder`. Aplikace a generátor jsou v Rustu; Windows EXE navíc obsahuje nástroje yt-dlp, Deno a FFmpeg pro média. Generování nevyžaduje další kompilátor ani externí skripty.

Hlavní aplikaci lze kompilovat i pro Linux podle [návodu](BUILD-LINUX.cs.md). Generátor instalačního EXE je určen pro Windows x64. Technické podrobnosti a izolované testování popisuje [anglický návod](SETUP-BUILDER.md).

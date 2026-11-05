# LIKE Video – változásnapló

A `like-video` ág minden saját változtatása ide kerül: mit, miért, hogyan teszteltük.
A legfrissebb bejegyzés van felül.

Visszaállás bármelyik mentési pontra: `git reset --hard <commit>` (a commit-azonosító minden bejegyzésnél szerepel).

---

## 2026-11-05 – #13 Claude-kapcsolat (MCP) tartósan; az eredeti frissítő kikapcsolva

**Claude Code kapcsolat:** a FilmCraft MCP-szervere eddig csak ebben a fejlesztői mappában élt
(`.mcp.json`, a fejlesztői `target\release` példányra mutatva), és a futó programhoz nem tudott
kapcsolódni, mert hiányzott a vezérlőkulcs. Beállítva a tulajdonos gépén (a tárolóban nincs titok):
- felhasználói környezeti változók: `FILMCRAFT_CONTROL_TOKEN` (véletlen, 32 bájtos kulcs) és
  `FILMCRAFT_CONTROL_PORT=9876`. Az asztali ikonról indított Like FilmCraft így magától fogadja a
  vezérlést, csak a gépen belül (127.0.0.1) és csak ezzel a kulccsal;
- Claude Code, felhasználói szint: `like-filmcraft` MCP-szerver →
  `%LOCALAPPDATA%\Like FilmCraft\filmcraft-cli.exe mcp --bridge 127.0.0.1:9876` (bármelyik
  mappából elérhető).
Élő próba: a Claude képernyőképet kért a futó programról, és kattintott benne.

**Az eredeti FilmCraft frissítője kikapcsolva.** Az élő próba közben a program felajánlotta:
„FilmCraft 0.17.1 – Update and restart”. Ez a gomb a hivatalos FilmCraftot telepítette volna
vissza, és a magyar felület meg a LIKE-eszközök elvesztek volna. CSERE (upstream):
`crates/ui-egui/src/updates_ui.rs` – `LIKE_EDITION`: nincs indításkori ellenőrzés és ajánlat; a
Súgó ▸ Frissítések keresése üzenetben elmondja, hogy a Like FilmCraft a saját telepítőjével frissül.
Új teszt: `like_edition_never_checks_upstream`. Az upstream újdonságait továbbra is mi olvasztjuk
be (lásd #10).

**Megjegyzés:** a felső sáv a projekt nevét mutatja. A bemutató projekt neve „FilmCraft – Demo
Project”, ez nem a program neve.

---

## 2026-11-05 – #12 Saját név: Like FilmCraft

**Miért:** a program a Névjegyben és az ablakcímben „FilmCraft 0.17.0”-nak nevezte magát, ezért
úgy tűnt, mintha az eredeti FilmCraft futna, nem a továbbfejlesztett változat.

- CSERE (upstream, 1 sor): `apps/filmcraft/src/main.rs` – az ablak címe „Like FilmCraft”.
- CSERE (upstream): `apps/filmcraft/build.rs` – az exe adatai (ProductName, FileDescription).
- CSERE (upstream, 2 sor + 1 sor): `crates/ui-egui/src/panels/dialogs.rs` – a Névjegy fejléce
  „Like FilmCraft”, verzió `0.17.0-like.12`, és egy mondat arról, hogy FilmCraft alapú.
  HOZZÁADÁS: `hu.tsv` (a mondat magyarul).
- HOZZÁADÁS: `like_tools.rs` – `PRODUCT_NAME` és `LIKE_RELEASE` (a LIKE-kiadás száma egy helyen;
  minden számozott bejegyzésnél eggyel nő). A telepítő verzióját is ebből veszi.
- **Szándékosan nem változott:** az eframe-alkalmazásazonosító („FilmCraft”), mert abból számolódik
  a beállítások mappája; átírásával elvesznének a meglévő beállítások. A Súgó menü „FilmCraft on
  GitHub” pontja az eredeti projektre mutat, ez helyes.

---

## 2026-11-05 – #11 Egygombos előkészítés, olvasható feliratidőzítés, munkaidő-mérés

Mentési pont előtte: `backup_20261105_elotte_like11` címke. Upstream fájl nem változott.

**ÚJ – Szekvencia ▸ LIKE ▸ Egygombos előkészítés** (`like.autoPrep`): leirat (ha még nincs, a
háttérben), durva vágás, az ismétlések és a szóbeli vágási jelek kivágása, magyar felirat, a túl
gyors feliratok javítása, hangtisztítás, végül az AI-javítás a háttérben, utána újabb
olvashatósági javítással. `"retakes": "mark"` esetén az ismétléseket csak megjelöli átnézésre.
Ha az AI-szolgáltató nem érhető el, a többi lépés megmarad, és hibaüzenet jelzi az okát.
CSERE: `like_tools.rs` – `mark_retakes` két részre vált: a csendes jelölés (`mark_retakes_quiet`)
és az üzenet. Az előkészítés így nem írja ki a „töröld a jelölőjét” tanácsot.

**ÚJ – Szekvencia ▸ LIKE ▸ Túl gyors feliratok javítása** (`like.fixReadingSpeed`,
`like_tools/readability.rs`): ami 17 karakter/mp-nél gyorsabb vagy 1 mp-nél rövidebb, azt előbb a
következő felirat előtti szünetbe nyújtja, majd az előző utániba. Legfeljebb 7 mp-ig nyújt, és
2 képkocka szünetet hagy a feliratok között, átfedés nélkül. Egy visszavonható lépés. Amit nem
lehet javítani, azt kijelöli.

**ÚJ – Szekvencia ▸ LIKE ▸ Munkaidő ezen a projekten** (`like.workTime`, `like_tools/worktime.rs`):
az asztali program magától méri az aktív munkát. Munka a szerkesztés és a lejátszás; az 5 percnél
hosszabb szünet nem számít. Fajtánként (vágás, felirat, hang, szín, grafika, megtekintés), naponként,
plusz a LIKE-feladatok gépideje. Tárolás: `<projekt>.fcproj.like-munkaido.json`, legfeljebb
percenként írva. A mentetlen projekt ideje az első mentéskor átkerül. A parancssori futás és a
tesztek nem mérnek (csak az automatikus mentést futtató munkamenet). A nap a FilmCraft saját helyi
időeltolódásával, chrono-val számolódik.

**Élő próba** (valódi motor és AI, 90 mp-es videó): az egygombos előkészítés 16 mp alatt lefutott.
Az eredmény azonos a lépésenkénti lánccal (szekvenciahossz, 13 felirat, hangtisztítás 50 Hz,
ismétlésjelölő nem maradt). A leghosszabb sor 42 karakter, a „LIKE CRM” helyes.

**Tesztek:** a LIKE-tesztek zöldek (54, köztük 13 új: időzítés, átfedésmentesség, ellenséges
értékek, egy visszavonási lépés, munkaidő-könyvelés, sérült fájl, helyi nap), a felület menü- és
fordítási tesztjei is zöldek. A motor teljes készlete: 942 zöld, 1 hibás. A hibás az ismert upstream teszt
(`single_frame_in_out_exports_warn_without_changing_the_output`, Windows-fájlzár). Tegnap, ezek
előtt a változtatások előtt is elbukott; nem a mi kódunk.

---

## 2026-11-05 – #10 Upstream frissítés, új Diktálás-motor, közös telepítő

**Upstream frissítés:** 23 új FilmCraft-commit beolvasztva ütközés nélkül (többek közt
H.264 High 10 export, Premiere-sablonok importja, hangmagasság megtartása sebességváltásnál,
mappák behúzása, elforgatott videó GPU-n). Mentési pont: `backup_20261105_elotte_upstream` címke.
HOZZÁADÁS: `hu.tsv` – 58 új felületszöveg magyarul, és egy megváltozott szöveg frissítve
(„Drop files or folders…”). A 19 megmaradt „hiányzó” a saját, eleve magyar LIKE-menüpontunk.

**LIKE Diktálás (2.4.3 PROBA3) beépítése:** a FilmCraft oldalon nem kellett kódot módosítani, a
hídprotokoll (`--subtitle-file`, 1. formátumverzió) változatlan. A FilmCraft a telepített motort
használja. Az új motor: Parakeet V3 a feliratokhoz, a hosszú hang darabolása a szüneteknél, a halk
beszéd kiemelése a hangfelismerés (VAD) előtt, a felismerés külön folyamatban (összeomlás után
újrapróbál). A Diktálás `like-wip-20260923` ága mostantól a privát GitHub-tárolóban is megvan.

**Élő próba a frissített programmal** (`filmcraft-cli run`, ablak nélkül):
- 90 mp-es videó, teljes lánc: durva vágás, 2 ismétlés és 2 szóbeli vágási jel kivágva,
  13 felirat, AI-javítás, hangtisztítás, négy export → 49 mp-es videó; 207 mp gépidő, ennek nagy
  része a négy export. A leghosszabb sor 42 karakter, a „LIKE CRM” név helyes.
- 10 perces hangfelvétel: leirat, durva vágás és 119 felirat összesen 21 mp alatt; a felirat
  a felvétel végéig tart, a leghosszabb sor 42 karakter, egyik felirat sem több két sornál.

**Közös telepítő (ÚJ):** `packaging/like/like-filmcraft.nsi` + `build-installer.ps1`. Upstream
fájl nem változott. Felhasználói telepítés a `%LOCALAPPDATA%\Like FilmCraft` mappába (nem kér
rendszergazdai jogot), asztali és Start menü ikon, útmutató, eltávolító. A „LIKE Diktálás
beszédmotor” összetevő a Diktálás telepítőjét csendes módban futtatja. A `/NODIKTALAS`
kapcsolóval csak a FilmCraft települ. Kimenet: `target\like-installer\Like-FilmCraft-<verzió>-x64-Telepito.exe`
(nem kerül a tárolóba).

**Csere a tulajdonos gépén:** a Like FilmCraft 0.17.0-like.10 telepítve, az asztali
„Like FilmCraft” ikon a telepített példányra mutat (eddig a fejlesztői `target\release`
mappára). A régi `filmcraft.exe` ikon törölve. A hivatalos FilmCraft 0.17.0 (MSI, Program Files)
a tulajdonos engedélyével (UAC) eltávolítva; helyette a Like FilmCraft fut.

---

## 2026-11-04 – #9 Feliratstílusok, Felirat munkaterület, helyes álló export

**Teljes működési próba valódi videón.** A tulajdonos 88 mp-es hangfelvételéből 90 mp-es MP4 készült
(színes háttérrel), és a teljes LIKE-lánc ablak nélkül végigfutott rajta: import, durva vágás,
ismétlések, felirat, AI-javítás, hangtisztítás, négy export és poszt-szöveg. 90 mp → 51 mp,
3–5 perc gépidő, ennek nagy része a négy export. Az exportok képkockáit egyenként ellenőriztük.

**A próba hibái és javításuk:**
- **Álló (9:16), négyzetes és 4:5 exportnál a felirat kilógott a képből.** A FilmCraft a 16:9-es
  szekvencia méretében égeti rá a feliratot, és csak utána vágja ki a célképet. HOZZÁADÁS:
  `like_tools.rs`, `reframed_export`. Csak az export idejére a szekvencia felveszi a célméretet,
  a klipek kitöltik a keretet, a LIKE-stílusú felirat pedig a képszélességhez méretezett,
  26 karakteres sorokra tördelt közösségi stílust kap. Az export a projekt saját másolatából
  dolgozik, utána a projekt, a visszavonási lista és a mentett állapot változatlanul visszaáll
  (teszt: `reframed_exports_leave_the_project_untouched`). A felhasználó saját feliratstílusát nem
  írja felül.
- **Az AI-javítás után egy sor 43 karakteres lett.** HOZZÁADÁS: `wrap_to` (a legkevesebb sorra,
  legfeljebb N karakterrel, egyenletesen, névelő nélkül a sor végén). CSERE: a javított szöveg
  `wrap_to(…, 42, eredeti sorszám, 3)`-mal tördelődik, és inkább 3 sor lesz, mint 42-nél hosszabb.

**Új:**
- `like.captionStyle.classic` / `like.captionStyle.social`: Szekvencia ▸ LIKE ▸ Feliratstílus.
- **Felirat munkaterület** (Ablak ▸ Munkaterületek ▸ Felirat): DaVinci-szerű feliratoldal.
  CSERE (upstream, minimális): `crates/ui-egui/src/dock.rs` `WORKSPACES` (10 → 11 elem, „Subtitles")
  és egy elrendezés-ág. HOZZÁADÁS: `hu.tsv` „Subtitles → Felirat”;
  `tests/it/workspaces_ui.rs` várt lista.

**Tesztek:** engine 916 zöld, a munkaterület- és menütesztek zöldek.
**Nyitott:** az ablakos `filmcraft.exe` újrafordítása a felhasználó gépén (a fejlesztői homokozó
nem olvashatja a beégetett PNG-ikonokat). A parancssori változat lefordult és tesztelve van.

---

## 2026-11-04 – #8 AI-feliratjavítás és poszt-szöveg élesben (OpenAI)

Élő próba a tulajdonos felvételén, új OpenAI-kulccsal (a régit az OpenAI elutasította).

**Eredmény:** az AI-javítás 11 feliratblokkot javított, a sorhossz legfeljebb 42 karakter maradt
(„Marról” → „Ma arról”, „döcspórról” → „időt spórol”, „lájkérm” → „LIKE CRM”). A poszt-szöveg
öt platformra elkészült, és megnyílt a Jegyzettömbben.

**Hibák és javításuk:**
- **Poszt-szöveg HTTP 400 OpenAI-jal.** A LIKE Diktálás `subtitle_repurpose::output_schema` és
  `text_cards::suggestion_schema` sémája dupla csomagolásban ment ki (az `llm_client` maga is
  becsomagolja). Az Anthropic szövegbe ágyazva kapta, ezért nem jött elő. Javítva a LIKE
  Diktálásban, regressziós teszttel.
- **A 403-as válasz „érvénytelen kulcsnak” látszott.** A LIKE Diktálás `classify_api_error`
  mostantól csak a 401-et kezeli hibás kulcsként. A 403 a szolgáltató indoklásával jelenik meg,
  a 401/403 indoklása pedig a naplóba kerül.
- **A többsoros motorhiba csonkán érkezett** („HTTP 400: {”). CSERE:
  `crates/engine/src/external_speech.rs`, `engine_error_message`: a `hiba:` utáni sorokat is
  hozzáveszi (legfeljebb 600 karakter). Két új teszt.
- **A Jegyzettömb fogva tartotta a CLI kimenetét,** amíg be nem zárták. CSERE:
  `crates/engine/src/like_tools.rs`, `open_document`: leválasztott stdio.
- A megszakítási teszt `std::time::Instant`-ot használt (wasm-szabály). CSERE: `web_time::Instant`.

**Tesztek:** engine 911 zöld. Egy upstream teszt
(`export_tests::single_frame_in_out_exports_warn_without_changing_the_output`) párhuzamos futásnál
Windows-fájlzár miatt néha elbukik, egyedül futtatva zöld. Nem a mi kódunk. LIKE Diktálás: 832 zöld.

---

## 2026-11-04 – #7 Javítások az első élő próba alapján

Élő próba: a tulajdonos 88 mp-es diktálása (szándékos „ööö”-kkel, szünetekkel, újrakezdett
mondattal), ablak nélkül végigfuttatva (`filmcraft-cli run`). Eredmény a javítás előtt:
87,6 → 58,2 mp; 15 szünet és 1 ismétlés kivágva; a teljes lánc 7 mp alatt.

**Mit javítottunk:**
- Take-felismerő: akkor is ismétlés, ha a felismerő másképp tagolta a szavakat („elsős
  legfontosabb” / „elsősleg fontosabb”) – `like_tools/repeats.rs`, `compare`.
- ÚJ: szóbeli vágási jelek („ezt vágjuk ki”, „ki kell vágni”, „elrontottam”, „ez egy hiba
  volt”, „újrakezdem”): az utasítás és az előtte lévő tagmondat piros jelölőt kap (mondatvégen
  túl nem nyúl); az egymás melletti jelek egy vágás. A kivágás az átfedő jelölőket összevonja.
- LIKE Diktálás (`d56111b`): sor és blokk nem végződhet névelőre/kötőszóra; az AI-javítás
  hibával áll le, ha egy adag sem sikerült (eddig csendben „kész”-t jelzett).

**Megállapítás a tulajdonosnak:** a LIKE Diktálásban beállított AI-kulcs érvénytelen vagy
visszavont – az AI-javításhoz és a poszt-szöveghez frissíteni kell.

**Teszt:** 39 FilmCraft- és 174 Diktálás-teszt zöld, köztük a próba szövegéből készült
regressziós tesztek.

---

## 2026-11-04 – #6 Hangtisztítás, platform-export, poszt-szöveg

**Mit:** Szekvencia ▸ LIKE ▸
- „Hangtisztítás” – minden hangklip párbeszéd típusú lesz, a beépített „Clean Up Noisy Dialogue”
  sablon (zaj, dübörgés, zúgás, dinamika), zúgás 50 Hz, majd hangosság-kiegyenlítés
  (`essentialSound.*` parancsok, saját DSP).
- „Export: YouTube (16:9) / TikTok-Reels-Shorts (9:16) / LinkedIn (1:1) / LinkedIn-Instagram
  (4:5) / mind a négy” – a beépített exportsablonokra épül (`file.exportMedia`), felirat
  ráégetve, ha van, −14 LUFS; a fájl a projekt mellé vagy a Dokumentumok\Like FilmCraft mappába.
- „Poszt-szöveg és hashtagek (AI)” – háttérfeladat; a LIKE Diktálás meglévő `SUB-REP` láncát
  hívja (`--repurpose`), a kész Markdown-fájlt a Jegyzettömb megnyitja.
- Upstream-fájl nem változott.

**Teszt:** 33 teszt zöld, köztük valódi 1:1 export a bemutató projektből és a hangtisztítás
beállításainak ellenőrzése. A Diktálás-oldalon 2 új teszt (`f63b1a2`).

---

## 2026-11-04 – #5 Ismételt mondatok (take-ek) megjelölése és kivágása

**Mit:** Szekvencia ▸ LIKE ▸ „Ismételt mondatok megjelölése” és „Megjelölt ismétlések kivágása”.
Az elhagyott próbálkozások piros tartományjelölőt kapnak (megjegyzésben: az elhagyott és a
megtartott szöveg); a felhasználó törli azoknak a jelölőjét, amelyeket megtartana; a kivágás csak
a megmaradt jelölőket vágja ki, egy visszavonható lépésben. Az utolsó változat mindig megmarad.
- ÚJ (átvett): `crates/engine/src/like_tools/repeats.rs` + `repeats_tests.rs` – a per-simmons/filmcraft
  fork determinisztikus take-felismerője (MIT OR Apache-2.0), forrásmegjelöléssel. LIKE-módosítás:
  magyar gyakori szavak és töltelékszavak; a `ripple_delete_ranges` hívás a mi alapunkhoz igazítva.
- HOZZÁADÁS: `like_tools.rs` – `like.markRetakes`, `like.cutMarkedRetakes`. Upstream-fájl nem változott.

**Teszt:** az átvett tesztek + 2 magyar felismerési teszt + 2 folyamatteszt (jelölés → kivágás;
törölt jelölő → a take megmarad). 30 teszt zöld.

**Kapcsolódó javítás a LIKE Diktálásban** (`98b5b92`): egy feliratsor sem lehet hosszabb 42
karakternél akkor sem, ha a blokk összhossza belefér (a próbán 45 karakteres sor jött ki).

---

## 2026-11-04 – #4 Háttérben futó LIKE-feladatok

**Mit:** a Magyar leirat, a Gyors durva vágás (ha még nincs leirat) és a Felirat AI-javítása
háttérfeladatként fut, a Folyamat panelen látszik és megszakítható; a megszakítás a motor
folyamatát is leállítja. Az eredmény egy visszavonható lépésben kerül a projektbe; a közben kézzel
átírt feliratot az AI-javítás nem írja felül.
- CSERE: `crates/engine/src/like_tools.rs` (közös `start_job` / `poll`, a `scene_detect` mintájára).
- CSERE: `crates/engine/src/external_speech.rs` (`run_engine_cancellable`, `transcribe_audio`).
- HOZZÁADÁS (upstream, 1 sor): `crates/engine/src/lib.rs` `poll_persistence` → `like_tools::poll(self);`.

**Miért:** leiratozás közben a program nem fagyhat le; hosszú videónál ez percekig tartana.
Az upstream #2517 / #3822 ugyanezt oldaná meg, de 7, illetve 27 fájlban ütközik a fő ággal.

**Teszt:** megszakítási teszt (futó folyamat leállítása 0,3 mp után), hiányzó motor hibaüzenete,
üzenetszöveg tizedesvesszővel; a motor 78 érintett tesztje zöld.

---

## 2026-11-04 – #3 Magyar kezelőfelület

**Mit:** a FilmCraft teljes felülete magyarul (3727 szöveg), új nyelvként a nyelvlistán.
- ÚJ: `crates/ui-egui/src/i18n/hu.tsv` (+ `hu.tsv.attribution`, sor az `ATTRIBUTION.md`-ben).
- MÓDOSÍTÁS (upstream-fájlok, a többi nyelv mintájára): `crates/ui-egui/src/i18n.rs`
  (`Language::Hu`), `crates/ui-egui/src/menus.rs` (Edit ▸ Language ▸ Magyar),
  `crates/ui-egui/src/i18n/tests.rs` (nyelvváltási teszt + betűkészlet-teszt ő/ű betűkre).

**Miért:** a tulajdonos fő kérése az egyszerű, magyar felület. A fordítás közösségi
hozzájárulásként az eredeti FilmCraft-projektnek is felajánlható (clean-room, MIT/Apache).

**Frissítési kockázat:** alacsony. Ha az upstream új nyelvet vesz fel, ugyanezekben a listákban
egy-egy sor ütközhet; percek alatt feloldható. Az új upstream-szövegek angolul jelennek meg,
amíg le nem fordítjuk őket.

**Teszt:** `cargo test -p filmcraft-ui-egui --lib i18n` (helyőrzők, nyelvváltás, betűkészlet).
Windows magyar nyelvi beállításnál a „Rendszer nyelve” magától a magyart választja.

---

## 2026-11-04 – #2 Külső beszédmotor és a Sequence ▸ LIKE menü

**Mit:**
- ÚJ: `crates/engine/src/external_speech.rs` – általános csatlakozó külső beszédfelismerő
  programhoz (WAV be, JSON ki, dokumentált 1. protokollverzió). A LIKE Diktálást a
  `FILMCRAFT_SPEECH_ENGINE` környezeti változó, a FilmCraft melletti `like-diktalas.exe`, vagy a
  `%LOCALAPPDATA%\LIKE Diktálás\` telepítés alapján találja meg.
- ÚJ: `crates/engine/src/like_tools.rs` – a Szekvencia ▸ LIKE menü parancsai:
  `like.quickCut` (Gyors durva vágás), `like.transcribe` (Magyar leirat),
  `like.createCaptions` (Magyar felirat), `like.correctCaptions` (AI-javítás),
  `like.moveWordNext` / `like.moveWordPrev` (szó áthelyezése a szomszédos feliratba),
  `like.openEngineSettings` (a motor beállításai).
- HOZZÁADÁS (upstream): `crates/engine/src/lib.rs` 2 sor (`pub mod …`),
  `crates/engine/src/commands.rs` 1 sor (`v.extend(crate::like_tools::commands())`).
- A LIKE Diktálás oldalán: `src-tauri/src/like_bridge.rs` (`--subtitle-file`,
  `--segment-words`, `--correct-captions`), commit `1b2de1c` a `like-wip-20260923` ágon.

**Miért:** a 6–8 órás videókészítés legnagyobb időfalói a durva vágás és a feliratozás. A
megoldás a FilmCraft meglévő parancsaira (`transcript.removeFillers`, `transcript.removePauses`,
`captions.*`) és a LIKE Diktálás meglévő láncaira épül; új algoritmust alig kellett írni.

**Teszt:** 11 egységteszt (FilmCraft) + 6 egységteszt (Diktálás), mind zöld. Élő teszt a
90%-os átadáskor (lásd `docs/LIKE-HASZNALATI-UTMUTATO.md`).

---

## 2026-11-04 – #1 NVIDIA Parakeet és a Create captions ablak (upstream PR-ok)

**Mit:** az upstream #3008 (Parakeet TDT v3, szószintű időzítés) és #3018 (Create captions ablak
mentett sablonokkal, szóról szóra felirat) beolvasztása, ütközés nélkül. Commit `9dbb5dc7`.

**Miért:** kész, közösségi megoldás; ha az upstream befogadja, nálunk ugyanazok a commitok vannak.

---

## 2026-11-04 – #0 Kiinduló állapot

**Mit:** saját fork és helyi munkapéldány a FilmCraftból, saját fejlesztési ággal.

| Elem | Érték |
|---|---|
| Fork | https://github.com/zslacus-oss/filmcraft |
| Eredeti projekt (`upstream`) | https://github.com/storytold/filmcraft |
| Helyi mappa | `C:\Claude-Code\Like Filmcraft` |
| Munkaág | `like-video` |
| Kiinduló upstream commit | `c9be1424` (FilmCraft 0.17.0 utáni `main`) |
| Licenc | MIT / Apache-2.0 |

**Miért az eredeti és nem a per-simmons fork:** az eredeti szinte naponta frissül; a per-simmons
fork 2735 committal lemaradt, és a GPU-gyorsítása csak macOS-en (Metal) működik. A magyar
feliratozás a saját LIKE Diktálás programból érkezik, kívülről (vezérlőcsatorna).

**Döntések:**
- Közzététel: minden platformra kézi feltöltés (TikTok, YouTube, Instagram, LinkedIn). Postiz nem
  kell, platform-fejlesztői regisztráció sem.
- Upstream kódhoz nem nyúlunk; minden saját fejlesztés kívülről, a vezérlőcsatornán / CLI-n át.

**Fordítás (Windows 10):**
- `cargo build --release -p filmcraft -p filmcraft-cli` → sikeres, 11 perc 15 mp.
- Eredmény: `target\release\filmcraft.exe` (71 MB), `target\release\filmcraft-cli.exe` (45 MB).
- Rust 1.97.1. A függőségek a projekten belüli `.cargo-home` mappába töltődnek
  (`CARGO_HOME`), mert a fejlesztői homokozó a `%USERPROFILE%\.cargo` mappát nem olvashatja.
  A mappa csak a helyi `.git/info/exclude`-ban van kizárva, az upstream `.gitignore` változatlan.

**Teszt:**
- A hivatalos 0.17.0 Windows-telepítő (aláírás: Learning Machines Inc., érvényes) a tulajdonos
  gépén elindult, és a tulajdonos szerint minden működik.
- `filmcraft-cli --version` → `filmcraft-cli 0.17.0`.

# LIKE Video – változásnapló

A `like-video` ág minden saját változtatása ide kerül: mit, miért, hogyan teszteltük.
A legfrissebb bejegyzés van felül.

Visszaállás bármelyik mentési pontra: `git reset --hard <commit>` (a commit-azonosító minden bejegyzésnél szerepel).

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

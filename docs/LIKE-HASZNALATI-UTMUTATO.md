# Like FilmCraft – rövid használati útmutató (tesztváltozat)

Cél: egy nyers beszédes videóból kész, magyar feliratos videó a lehető legkevesebb kézi munkával.

## Telepítés

Egy telepítő tesz fel mindent: `Like-FilmCraft-<verzió>-x64-Telepito.exe`. Nem kér
rendszergazdai jogot.

- A program a `%LOCALAPPDATA%\Like FilmCraft\` mappába kerül, az asztalon és a Start menüben
  „Like FilmCraft” ikonnal.
- A „LIKE Diktálás beszédmotor” összetevő a magyar beszédmotort telepíti vagy frissíti. A
  beállításaid, a szótárad és a letöltött modellek megmaradnak.
- Frissítés: futtasd az újabb telepítőt. A projektjeid nem változnak.
- Eltávolítás: Start menü ▸ Like FilmCraft ▸ Like FilmCraft eltávolítása. A LIKE Diktálás
  külön program, a saját eltávolítójával távolítható el.

## Mi kell hozzá

1. **Like FilmCraft** (ez a program).
2. **LIKE Diktálás 2.4.3 vagy újabb** (ez a magyar beszédmotor; a közös telepítő felteszi). A
   FilmCraft magától megtalálja a `%LOCALAPPDATA%\LIKE Diktálás\` mappában.
3. A LIKE Diktálásban beállítva: a személyes szótár (ide kerüljenek a márka- és terméknevek, pl.
   „LIKE CRM"), és – az AI-javításhoz – egy AI-szolgáltató kulccsal. A feliratokhoz a Diktálás a
   Parakeet V3 modellt használja (szóidővel); ha nincs letöltve, a Diktálás modellkezelőjében töltsd
   le. A hosszú felvételeket (fél óra is) a szüneteknél darabolva dolgozza fel.

## A munkafolyamat

| Lépés | Hol | Mit csinál |
|---|---|---|
| 1. Importálás | Fájl ▸ Importálás… vagy húzd be a videót | a nyers felvétel a projektbe kerül |
| 2. Szekvencia | húzd a klipet az idővonalra | elkészül a szekvencia |
| 3. **Gyors durva vágás** | Szekvencia ▸ LIKE ▸ Gyors durva vágás | magyar leirat készül, majd kivágja a töltelékhangokat („ö", „öö", „hmm", „izé"…) és a 0,6 mp-nél hosszabb szüneteket, 0,12 mp levegőt hagyva |
| 4. **Ismételt mondatok** | Szekvencia ▸ LIKE ▸ Ismételt mondatok megjelölése | minden elrontott, újrakezdett mondat korábbi próbálkozása piros tartományjelölőt kap; az utolsó változat érintetlen. Amelyiket meg akarod tartani, annak töröld a jelölőjét |
| 5. Ismétlések kivágása | Szekvencia ▸ LIKE ▸ Megjelölt ismétlések kivágása | csak a megmaradt piros jelölők alatti részt vágja ki (Ctrl+Z-vel visszavonható) |
| 6. Finom vágás | idővonal, Szöveg panel ▸ Leirat | szöveg alapján: jelöld ki a felesleges mondatot, és Kivonás |
| 7. **Magyar felirat** | Szekvencia ▸ LIKE ▸ Magyar felirat készítése | a megvágott leiratból magyar szabályú felirat (42 karakter, 2 sor, névelő nem marad sor végén) |
| 8. **AI-javítás** | Szekvencia ▸ LIKE ▸ Felirat AI-javítása | helyesírás, ragozás, szótárad szakszavai; az időzítés nem változik |
| 9. Kézi simítás | Ablak ▸ Munkaterületek ▸ **Felirat** (balra a feliratszerkesztő, középen a kép, alul az idővonal) | szöveg átírása; ✂ kettévágás, összevonás; „Utolsó szó át a következő feliratba" / „Első szó vissza az előző feliratba" |
| 10. **Hangtisztítás** | Szekvencia ▸ LIKE ▸ Hangtisztítás | zajszűrés, dübörgés- és zúgásszűrés (50 Hz), hangosság-kiegyenlítés minden hangklipen |
| 11. Stílus | Szekvencia ▸ LIKE ▸ Feliratstílus: klasszikus / közösségi, vagy kézzel: Szöveg panel ▸ Feliratok, stíluscsík | klasszikus: alul, áttetsző sávon (YouTube); közösségi: nagy, félkövér, körvonalas. Saját stílust is beállíthatsz, azt az export nem írja felül |
| 12. **Export** | Szekvencia ▸ LIKE ▸ Export: YouTube (16:9) / TikTok-Reels-Shorts (9:16) / LinkedIn (1:1) / LinkedIn-Instagram (4:5), vagy „mind a négy platformra" | ráégetett felirattal (ha van), −14 LUFS hangossággal. Az álló, négyzetes és 4:5-ös változatnál a kép kitölti a keretet, a felirat pedig magától rövidebb sorokra tördelődik, nagyobb betűvel, a TikTok/Reels gombjai fölött; a projekted ettől nem változik. A fájlok a projekt mellé, vagy a Dokumentumok\Like FilmCraft mappába kerülnek |
| 13. **Poszt-szöveg** | Szekvencia ▸ LIKE ▸ Poszt-szöveg és hashtagek (AI) | platformonként cím, nyitómondat, leírás, hashtagek egy fájlban; a Jegyzettömb megnyitja, onnan másold |
| 14. Feltöltés | kézzel, a platformon | |

A LIKE-lépések mind visszavonhatók (Ctrl+Z).

## Mérd az időt

A cél a 6–8 órás munka drasztikus csökkentése. Egy-két videónál írd fel, mennyi ideig tartott:

| Szakasz | Lépések | Perc |
|---|---|---|
| Gépi előkészítés | 1–5. (import, durva vágás, ismétlések) | |
| Kézi vágás | 6. | |
| Felirat | 7–9. | |
| Kép és hang | 10–11. | |
| Export és poszt-szöveg | 12–13. | |
| **Összesen** | | |

Ahol a legtöbb perc megy el, oda kerül a következő fejlesztés.

## Ha valami nem megy

- **A LIKE menüpontok szürkék:** nincs nyitott szekvencia, vagy a LIKE Diktálás nincs telepítve.
- **„the speech engine failed…" üzenet:** a LIKE Diktálásban nincs letöltve vagy kiválasztva
  beszédmodell, vagy nincs AI-szolgáltató (a javításhoz). Nyisd meg: Szekvencia ▸ LIKE ▸ LIKE motor
  beállításai.
- **A durva vágás a szavakba is belevág:** a szóidő becsült volt (nincs szóidős modell). Vond
  vissza (Ctrl+Z), töltsd le a szóidős modellt a LIKE Diktálásban, és futtasd újra.
- **Az AI-javítás vagy a poszt-szöveg hibát ad:** az üzenet megmondja az okát (kulcs, keret,
  jogosultság). A kulcsot a LIKE Diktálás ▸ Beállítások ▸ AI-szolgáltató alatt cseréld.
- **Egy név rosszul jelenik meg** (pl. „lájkérm"): vedd fel a helyes alakját a LIKE Diktálás
  személyes szótárába; a következő AI-javítás már azt használja.

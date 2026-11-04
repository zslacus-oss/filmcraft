# Like FilmCraft – rövid használati útmutató (tesztváltozat)

Cél: egy nyers beszédes videóból kész, magyar feliratos videó a lehető legkevesebb kézi munkával.

## Mi kell hozzá

1. **Like FilmCraft** (ez a program).
2. **LIKE Diktálás 2.4.0 vagy újabb** telepítve (ez a magyar beszédmotor). A FilmCraft magától
   megtalálja a `%LOCALAPPDATA%\LIKE Diktálás\` mappában.
3. A LIKE Diktálásban beállítva: a beszédmodell (Whisper Large v3 Turbo), a személyes szótár, és –
   az AI-javításhoz – egy AI-szolgáltató kulccsal. Szóidőhöz ajánlott egy szóidős modell
   (Parakeet / Nemotron) letöltése is; enélkül a szóidő becsült lesz.

## A munkafolyamat

| Lépés | Hol | Mit csinál |
|---|---|---|
| 1. Importálás | Fájl ▸ Importálás… vagy húzd be a videót | a nyers felvétel a projektbe kerül |
| 2. Szekvencia | húzd a klipet az idővonalra | elkészül a szekvencia |
| 3. **Gyors durva vágás** | Szekvencia ▸ LIKE ▸ Gyors durva vágás | magyar leirat készül, majd kivágja a töltelékhangokat („ö", „öö", „hmm", „izé"…) és a 0,6 mp-nél hosszabb szüneteket, 0,12 mp levegőt hagyva |
| 4. Finom vágás | idővonal, Szöveg panel ▸ Leirat | szöveg alapján: jelöld ki a felesleges mondatot, és Kivonás |
| 5. **Magyar felirat** | Szekvencia ▸ LIKE ▸ Magyar felirat készítése | a megvágott leiratból magyar szabályú felirat (42 karakter, 2 sor, névelő nem marad sor végén) |
| 6. **AI-javítás** | Szekvencia ▸ LIKE ▸ Felirat AI-javítása | helyesírás, ragozás, szótárad szakszavai; az időzítés nem változik |
| 7. Kézi simítás | Ablak ▸ Munkaterületek ▸ Feliratok és grafika | szöveg átírása; ✂ kettévágás, összevonás; „Utolsó szó át a következő feliratba" / „Első szó vissza az előző feliratba" |
| 8. Stílus | Szöveg panel ▸ Feliratok, stíluscsík | betűtípus, méret, szín, körvonal, háttérdoboz, pozíció |
| 9. Export | Fájl ▸ Exportálás | YouTube / 9:16 közösségi sablon; „Feliratok ráégetése a videóra" |
| 10. Feltöltés | kézzel, a platformon | |

A 3., 5. és 6. lépés mind visszavonható (Ctrl+Z).

## Ha valami nem megy

- **A LIKE menüpontok szürkék:** nincs nyitott szekvencia, vagy a LIKE Diktálás nincs telepítve.
- **„the speech engine failed…" üzenet:** a LIKE Diktálásban nincs letöltve vagy kiválasztva
  beszédmodell, vagy nincs AI-szolgáltató (a javításhoz). Nyisd meg: Szekvencia ▸ LIKE ▸ LIKE motor
  beállításai.
- **A durva vágás a szavakba is belevág:** a szóidő becsült volt (nincs szóidős modell). Vond
  vissza (Ctrl+Z), töltsd le a szóidős modellt a LIKE Diktálásban, és futtasd újra.
- A leiratozás alatt a program néhány tíz másodpercre „megállhat" – ez a jelenlegi változatban
  várható (a háttérben futtatás a következő kör feladata).

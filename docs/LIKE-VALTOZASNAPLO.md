# LIKE Video – változásnapló

A `like-video` ág minden saját változtatása ide kerül: mit, miért, hogyan teszteltük.
A legfrissebb bejegyzés van felül.

Visszaállás bármelyik mentési pontra: `git reset --hard <commit>` (a commit-azonosító minden bejegyzésnél szerepel).

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
| Első saját commit | `0f077a61` – `docs/LIKE-CLAUDE.md` (fejlesztési szabályok) |
| Licenc | MIT / Apache-2.0 |

**Miért az eredeti és nem a per-simmons fork:** az eredeti szinte naponta frissül; a per-simmons
fork 2735 committal lemaradt, és a GPU-gyorsítása csak macOS-en (Metal) működik. A magyar
feliratozás a saját LIKE Diktálás programból érkezik, kívülről (vezérlőcsatorna).

**Döntések:**
- Közzététel: minden platformra kézi feltöltés (TikTok, YouTube, Instagram, LinkedIn). Postiz nem
  kell, platform-fejlesztői regisztráció sem.
- Upstream kódhoz nem nyúlunk; minden saját fejlesztés kívülről, a vezérlőcsatornán / CLI-n át.

**Fordítás (Windows 10, Ryzen 5 3600X, 32 GB RAM):**
- `cargo build --release -p filmcraft -p filmcraft-cli` → sikeres, 11 perc 15 mp.
- Eredmény: `target\release\filmcraft.exe` (71 MB), `target\release\filmcraft-cli.exe` (45 MB).
- Rust 1.97.1. A függőségek a projekten belüli `.cargo-home` mappába töltődnek
  (`CARGO_HOME`), mert a fejlesztői homokozó a `%USERPROFILE%\.cargo` mappát nem olvashatja.
  A mappa csak a helyi `.git/info/exclude`-ban van kizárva, az upstream `.gitignore` változatlan.

**Teszt:**
- A hivatalos 0.17.0 Windows-telepítő (aláírás: Learning Machines Inc., érvényes) a tulajdonos
  gépén elindult, és a tulajdonos szerint minden működik.
- `filmcraft-cli --version` → `filmcraft-cli 0.17.0`.

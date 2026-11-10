# Tessera demo video — shot script and recording instructions

One take, recorded with OpenScreen (display capture), edited in
OpenScreen (zooms, captions, 16:10 frame), exported to
`website/public/media/tessera-demo.mp4`.

House style: lowercase captions, plain words, no music, no voiceover.
The drawn cursor and the zooms carry the energy.

## Verified query list (probed against the live index, 2026-11-10)

Every query below returns the named file at rank 0 unless noted.

| Lens | Query | Expected top hit |
|---|---|---|
| pictures | a dog in a blanket | a-dog-wrapped-up-into-a-blanket-82551910.jpg |
| pictures | people shopping inside a mall | crowded-shopping-mall.jpg |
| pictures | a cup of espresso on a wooden table | espresso-cup.jpg |
| pictures | sailboats in turquoise water | sailboat-turquoise-islands.png |
| pictures | hot air balloons in the sky | hot-air-balloons.jpg |
| words | CrashLoopBackOff | screenshots/pods.png |
| words | boarding pass | screenshots/boarding.png |
| words | total amount | screenshots/hotel.png |
| documents | sourdough | documents/recipe-sourdough.md |
| documents | itinerary | documents/itinerary.pdf |

Corpus gap filled for this take: `crowded-shopping-mall.jpg` ("Eaton
Center boxing day" by 松林Ｌ, CC BY 2.0, via Openverse/Flickr; credit in
`C:/Users/akash/tmp/tessera-demo-assets/CREDITS-mall.md`). It was indexed
with a manual rescan because the watcher pauses on battery.

## Prep (done before the take)

1. App rebuilt and running with CDP on port 9222; models warm.
2. Tessera maximized; theme light; query empty; home wall showing.
3. Firefox, Qoder and the OpenScreen bar minimized so the display
   capture shows only Tessera and the taskbar.
4. OpenScreen MCP bridge alive on port 47899 (`os-bridge.mjs`).
5. Watcher may stay paused on battery; nothing new needs indexing
   during the take.

## Recording rules

- `startRecording { display: 0, cursor: "editable-overlay" }`. Display,
  not window: the take keeps the desktop wallpaper and taskbar.
- Clicks are REAL mouse clicks from a DPI-aware SendInput helper
  (`tessera-mouse.ps1`), not computer-use: computer-use paints a HUD in
  the recording. The helper glides the pointer, so the take has real
  pointer telemetry for the zooms below.
- Typing is CDP only, word by word at a readable speed (about 55 ms per
  character plus a beat after each word). Never paste the query in one
  paste. Never OS keystrokes into Tessera.
- Open results on camera: in pictures, open one tile after the first
  query and one after the last; in words, open the first evidence card
  (the crop where the searched word is marked) after the first two
  queries. Close each sheet before moving on.
- Deliberate move-then-click (zoom moments): the first click on the
  search bar, the pictures→words lens click, and both theme buttons.
  Other switches are direct clicks.
- The add folder button is NOT clicked and no picker is shown. The
  pointer only moves near it, then leaves, so the zoom can land on it.
- Log every action to `C:/Users/akash/tmp/tessera-take.jsonl`
  (`{at, kind, label}`) so the edit can place zooms and captions.

## Shot list

| # | Scene | Action | Hold | Cursor / zoom |
|---|---|---|---|---|
| 1 | home | light theme, empty query, picture wall | 3 s | rest; no zoom in first seconds |
| 2 | pictures | click pictures lens, then click the search bar | 0.5 s | zoom on the search-bar click |
| 3 | pictures q1 | type "a dog in a blanket" | 2.6 s | cursor rests in the field |
| 4 | pictures q2 | retype "people shopping inside a mall" | 2.6 s | no new click |
| 5 | pictures q3 | retype "a cup of espresso on a wooden table" | 2.4 s | no new click |
| 6 | pictures q4 | retype "sailboats in turquoise water" | 2.4 s | no new click |
| 7 | pictures q5 | retype "hot air balloons in the sky" | 2.4 s | no new click |
| 7b | preview 2 | click the first tile, show it, close it | 2.2 s | zoom on the tile |
| 8 | preview | after q1: click the first tile, show the proof sheet, close it | 2.2 s | zoom on the tile |
| 9 | words | click the words lens, click the field | 0.5 s | zoom on the lens click |
| 10 | words q1 | type "CrashLoopBackOff", open the first evidence card (the word is marked in the crop), close it | 2.6 s + 2.4 s | zoom on the card |
| 11 | words q2 | retype "boarding pass", open the first evidence card, close it | 2.4 s + 2.2 s | zoom on the card |
| 12 | words q3 | retype "total amount" | 2.4 s | |
| 13 | documents | click the documents lens (direct) | 0.4 s | |
| 14 | documents q1 | type "sourdough" | 2.4 s | recipe row with snippet |
| 15 | documents q2 | retype "itinerary" | 2.4 s | itinerary.pdf row |
| 16 | settings | click settings (direct) | 1.8 s | folders, look, models visible |
| 17 | add folder | move the pointer near "add folder" and hold, never click | 1.8 s | zoom on the button; no dialog |
| 18 | theme dark | click "dark" | 2.2 s | deliberate move + click, zoom |
| 19 | theme light | click "light" | 1.6 s | deliberate move + click, zoom |
| 20 | shortcuts | scroll the shortcuts section into view | 2.2 s | |
| 21 | home again | click back | 2 s | |
| 22 | finale | rest on the home wall | 2.8 s | |

Add-folder note: the button is shown and the zoom lands on it, but the
click is skipped. No folder is added, no dialog opens, and no personal
path is shown.

## Captions (one per scene, bottom centre, rise-in)

Placed at x 50, y 89, size 38, colour #f5f2ea, plate
rgba(12,14,18,0.72).

- home (0.6–4.8): "tessera. search your files, on your machine."
- pictures (5.4–8.4): "pictures: describe it loosely. the wall answers."
- preview (12.4–15.7): "the match, with the proof on it."
- words (41.2–43.9): "words: find text inside pictures."
- evidence (47.5–50.1): "the exact line, marked in the picture."
- documents (63.9–66.4): "documents: pdf, word and text."
- settings (72.9–75.9): "settings: folders, look, models."
- add folder (77.2–79.1): "point it at any folder. it reads, then watches."
- theme (79.7–82.5): "light, dark, or follow the system."
- shortcuts (86.4–90.1): "keyboard first. models on device."
- finale (95.2–99.2): "1,091 pictures. 14 documents. no cloud."

## Edit plan

1. `stopRecording { title: "Tessera demo" }` → project + checkpoint.
2. Remove the zooms stopRecording planned. Clicks less than 1.5 s apart
   merge into long pans that follow the pointer and crop the window.
3. Add ten zooms by hand: 1.4×, focusMode manual, so the whole window
   stays in frame and the subject sits off centre when useful.
   - 5.6–8.0 pictures lens and field
   - 12.6–15.6 preview sheet (cx 0.62)
   - 41.0–43.8 words lens and field
   - 47.3–50.0 evidence panel (cx 0.58)
   - 63.9–66.3 documents field
   - 72.8–75.8 settings
   - 77.0–79.0 add folder button
   - 79.5–82.4 dark, 83.2–85.3 light
   - 86.3–90.0 shortcuts
4. `setAspectRatio "16:10"` (the recording's native 8:5 shot: a 16:9
   frame cuts the top taskbar). `setEditorSettings` padding 4, radius
   14, wallpaper #0e1013, shadow 0.4; cursor studio-ink size 2.0,
   smoothing 0.5, motion blur 0.15, click bounce 0.6, click impact on,
   click sound off.
5. Captions from the table above.
6. `exportVideo { quality: "good", fps: 60 }` to
   `C:/Users/akash/tmp/tessera-demo.mp4`.
7. Verify with `getFrames`: home at 0.3 and 2, pictures at 6.5, preview
   at 13.2 and 14, words at 42, evidence at 48, documents at 64.5 and
   66, settings at 73.5, add folder at 78, dark at 80.2, light at 81,
   shortcuts at 87 and 88, finale at 97 and 99. Check each frame keeps
   the full window, the caption, and the expected result.
8. Copy the MP4 to `website/public/media/tessera-demo.mp4` (left
   uncommitted for review). Restore the minimized windows afterwards.

## Safety

- No folder is added or removed; no Remove/Rebuild dialog is confirmed.
- No network at query time; the only downloads were the permissive
  stock photos, done before the take.
- Nothing is pushed or posted.
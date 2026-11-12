---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: []
---

# Surface brief: KeibaScope app shell (all four screens)

Mode: Operate. Desktop app (Tauri), 1440×920 default, min 1024×700. Light, daytime use.

Audience and job: the author alone, on race day before post time, deciding the anchor horse, the opponents, the tickets and the amounts within a few minutes. First thing to read within 10 seconds: the marks (◎○▲☆△) and the key horses. Must keep: racing's own mark symbols and the eight frame (waku) colors. Not required to keep: the old four-tab layout or the single giant table.

Unresolved: none at start.

## Direction contract

THESIS: Read the field the way it looks on the track: the frame-cap colour and saddle-cloth number are each horse's face, and the predicted order is a row of saddle cloths. Refuses the category default of a grey admin table with one blue accent and cards inside cards.

OWN-WORLD: Warm daylight ground #F7F6F2, ink #16181D, a royal-blue silks shell #1D3FA8 as the header band. The eight waku colours appear only as saddle cloths (condensed bold numerals, Barlow Condensed) with an ink keyline. Marks in red ink #C0392B. Violet #7A2BD9 is reserved for "in my tickets" and nothing else. Estimated odds wear a dashed "推定" frame. Noto Sans JP for text. Flat surfaces, hairline rules, no gradients, no nested cards.

STORY: The visitor opens a race, sees the order as a strip of saddle cloths with marks, reads the ◎ horse at display scale, scans the table for value, adjusts weights or budget and watches the strip reorder, then reads the tickets and amounts on the right and buys outside the app.

FIRST VIEWPORT: Blue header band: brand, four tabs (出馬表 / 開催一覧 / 買い目 / 回収率), race-ID input with the analyse button on the right. Under it the race title line at 30px with grade and conditions. Then the saddle-cloth strip across the full width (all runners in predicted order, mark above, index below, violet ring on ticket horses). Below, a two-column work area: left the ◎ lead line at display scale then the entry table; right a fixed 380px ticket panel (strategy segmented control, budget, tickets with saddle-cloth chips, odds, amount, total). Signature move: the saddle-cloth strip, which reorders in place with a short FLIP motion when weights change and links hover/selection to the table row.

FORM: Grounded candidate 3 of 7 (枠帽と勝負服). seed:90eaeab8ae65abda

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

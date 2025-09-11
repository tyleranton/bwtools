# bwtools – Feature Ideas

This file tracks potential improvements and extensions for bwtools. These build on the existing TUI, cache parsing, and bw-web-api-rs integration.

## Opponent Insights
- Multi‑toon summary details: show highest rating + gateway per toon (already computed), add tier/league badges if exposed or derivable.
- Recent matches preview: fetch recent results for detected opponent; show W/L and map names.
- One‑key copy/export: write a text snippet of opponent toons + ratings to a file for quick sharing or OBS overlays.

## Self Profile Tools
- Auto‑load all self profiles on connect and populate `own_profiles` (enable robust self filtering even before a match).
- Live rating trend: cache polled ratings and render a small sparkline in Status to show momentum.

## History & Persistence
- Match history log: append each detected opponent, their toons + ratings, and timestamp to JSON/CSV.
- History view: browse and filter past opponents inside a new “History” tab.
- Opponent notebook: user tags (e.g., “cheese”, “macro”), displayed on re‑detection.

## UI/UX Improvements
- Tabs/panels: add tabs for Main, Profiles, History, and Debug with hotkeys.
- Scrollable lists: add scroll to opponent toons list; optional help/hints toggle.

## OBS / Streaming
- Overlay text files: write “Self”, “Opponent”, and “Opponent toons” to `./overlay/` for OBS text sources.
- Mini HTTP endpoint (feature‑flagged): serve JSON status for external tools and overlays.

## Performance & Robustness
- Smarter cache scans: track latest `creation_time` per key and only process newer entries; shrink window after steady state.
- API backoff: exponential backoff and small per‑minute cap on API calls to avoid hammering when failures occur.

## Search & Lookup
- Leaderboard lookup: integrate `get_leaderboard` and add a view to browse leaderboards per region.
- Profile search: manual query for a toon’s `scr_tooninfo` off‑session.

## Misc / Future
- Config surface: env/CLI overrides for cache dir, tick rate, windows, and polling interval.
- Export/import settings and history.
- Tests: unit tests for URL parsing helpers and rating extraction.

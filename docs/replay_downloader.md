# Replay Downloader Feature Outline

## Goal
Provide an in-app workflow to fetch StarCraft: Remastered replays for a specific player and matchup, analyze their duration with `screp`, and save only meaningful games using an informative naming scheme.

## User Inputs
- **Player**: primary toon name the user wants to study.
- **Matchup**: desired race vs race combination (e.g., PvZ, TvT). Could be derived from player race + opponent race filters.
- **Replay Count**: maximum number of qualifying replays to download.

## Replay Data Source
- `ScrToonInfo` (from the existing `get_toon_info` calls) gives us the authoritative list of a player's toons and gateways; we can reuse that context to request richer profile data for the specific toon/gateway combination.
- The existing `ApiHandle::get_scr_profile` call already returns a `ScrProfile` payload for that toon (see `api.rs`).
- `ScrProfile.replays: Vec<Replay>` exposes replay entries, each containing:
  - `Replay.link`: identifier used to load full match data; the TypeScript sample passes it to `matchMakerGameInfoPlayerInfo`.
  - `Replay.create_time`: unix timestamp (seconds) that we can sort/filter on.
  - `Replay.attributes`: `ReplayAttributes` bundle with strings such as:
    - `replay_player_names`, `replay_player_races`, and `replay_player_types` (comma-delimited list of participants/races). The TS code treats them as `replay.players` / `replay.races` strings.
    - `game_id`, `game_save_id`, `map_title`, `game_type`, etc. useful for dedupe or presentation.
- To convert a replay entry into a concrete download URL we need to make a second request: `matchmaker-gameinfo-playerinfo/{match_id}` (surfaced by `ApiHandle` via `bw-web-api-rs`'s `get_matchmaker_player_info`). Its response (`MatchmakerPlayerInfo`) contains `replays: Vec<MatchmakerReplay>`; each replay in that list has:
  - `url`: actual download endpoint for the `.rep` content.
  - `md5`: digest suitable for dedupe.
  - `create_time`: timestamp we can use to pick the newest entry when two URLs are returned (mirroring the TS logic).
  - `attributes`: another `ReplayAttributes` bag aligned with the profile entry.

## High-Level Flow
1. **Request Replays**
   - Fetch `ScrProfile` for the target toon/gateway and pull its `replays` list.
   - Pre-filter by matchup: normalize the requested matchup into both `RaceA,RaceB` and `RaceB,RaceA` strings (mirroring the TS helper) and keep only entries whose `replay_player_races` value matches either ordering.
   - Respect the requested count, capped at the API limit (profile payload currently exposes up to 20 entries).
2. **Prepare Downloads**
   - For each candidate replay, call `get_matchmaker_player_info(replay.link)` to retrieve `MatchmakerReplay` entries and select the freshest URL (compare `create_time`).
   - Extract player names, races, and winner from the profile attributes to drive naming and later analysis.
3. **Deduplicate**
   - Check existing downloads using stable identifiers (`MatchmakerReplay.md5`, `Replay.attributes.game_id`, or filename collisions). Skip and optionally log duplicates.
4. **Download Step**
   - Stream replay binaries for the remaining candidates to a staging directory, handling retries and ensuring we clean up partial files on failure.
5. **screp Analysis**
   - Run `screp -overview` (or similar) on each downloaded replay to obtain:
     - Player names and races.
     - Game duration in seconds.
6. **Filter**
   - Drop any replay where the reported game length is 120 seconds (2 minutes) or shorter.
7. **Finalize Storage**
   - Rename surviving replays using the format `player1(race)_vs_player2(race).rep`; if a name already exists, append a numeric suffix (`-1`, `-2`, ...) before the extension.
   - Sanitize names for filesystem safety (strip slashes, trim whitespace, guard against reserved names on Windows).
   - Build the destination path under the StarCraft replay root (`Documents/StarCraft/Maps/Replays` on Windows; `$HOME/.wine-battlenet/drive_c/users/$USER/Documents/StarCraft/Maps/Replays` on Linux/Wine). Within that root create `bwtools/<ProfileName>/<Matchup>/` and place the replay there (e.g., `.../bwtools/By.SnOw1/PvT/player1(Protoss)_vs_player2(Terran).rep`).
   - Update the dedupe manifest (e.g., store MD5s or match IDs) once the file is successfully persisted; the manifest lives inside a hidden folder under the `bwtools` replay directory (e.g., `.../bwtools/.meta/manifest.json`). Use a single manifest file keyed by replay identifiers for fast lookups.

## Requirements & Considerations
- Ensure we never process or redownload replays that already exist locally (use `md5`, match IDs, or persisted filenames).
- Handle `screp` failures gracefully (retry or log and skip) without blocking other downloads.
- Preserve original casing for player names in filenames while normalizing races (e.g., `Protoss`, `Terran`, `Zerg`).
- Sanitize filenames to remove filesystem-unsafe characters and obey OS-specific constraints.
- Maintain a manifest of fetched replay IDs/MD5s to support deduplication across sessions and write it to a hidden `.meta` folder inside the `bwtools` replay directory; implement it as a single JSON file keyed by replay identifier for quick membership checks.
- Apply matchup filtering before download to save bandwidth.
- Respect API rate limits; consider backoff if matchmaker detail calls fail.
- Ensure directory creation respects OS differences: Windows uses `%USERPROFILE%/Documents/StarCraft/Maps/Replays`, Wine uses `$HOME/.wine-battlenet/drive_c/users/$USER/Documents/StarCraft/Maps/Replays`.

## Open Questions
- UI surface (new panel vs. command palette action) and progress reporting.
- Whether users can queue multiple matchups/players at once or run serially.

## UI Concepts
- **Dedicated Panel (Preferred)**: introduce a new "Replays" view listing queued downloads and history (similar layout to Debug view) with scrollable logs and per-job status. Access via a keyboard shortcut (e.g., `Ctrl+R`) or footer hint; reuse panel sections for input form (top) and progress/logs (bottom).
- **Background Notifications**: irrespective of entry point, surface completion/toast messages in the Status panel (e.g., append a line "Replay download complete: 5 saved" with optional error counts) to keep users informed while they switch views.

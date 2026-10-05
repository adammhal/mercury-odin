# Review screen: what the Odin plugin needs to wire up

Built on branch `pc` (PC first). The shared parts live in `plugin/src` and the engine, so the Odin plugin can use them.

## What exists
- **Engine** (`mercuryd`, all platforms):
  - `GET /sgdb/{appid}/{slot}` -> `{ options: [{url, thumb, score, width, height, author}] }` from SteamGridDB. Slots: 0 cover, 1 hero, 2 logo, 3 wide. Needs `sgdb_key` in config (saved through `PUT /config`; `sgdb_key_set` is echoed back, never the key).
  - `POST /art/resolve` `{appid, choices: {"0": url, ...}}` -> `{ assets: [[slot, ext, base64]] }`: Steam's store art with the chosen SteamGridDB images swapped in. Same shape as `GET /steam/art/{id}`, so it feeds `SteamClient.Apps.SetCustomArtworkForApp(shortcut, data, ext, slot)` directly.
  - Config: `review_art` (bool, default true), `sgdb_key`.
  - Job state `review` (`JobState` in `plugin/src/api.ts`). Windows jobs stop there after install until `POST /jobs/{id}/confirm` `{name, art}`. The Unix path does not enter `review` yet.
- **Shared UI**: `plugin/src/pages/Review.tsx` (route `/mercury/review/:id`, job id), `plugin/src/review.ts` (`setReviewHandler`, `confirmReview`). Title field, four art slots, a SteamGridDB picker modal, "Add to Steam" and "Not now".
- **Shim**: the PC shim now exports `ModalRoot` (Decky has it natively).

## To do on the Odin
1. Register the route in `src/index.tsx`: `/mercury/review/:id` -> `<Review />`.
2. In the plugin's job watcher: when a job reaches `ready` (install finished), open the Review page instead of calling `addToSteam` straight away (respect `config.review_art`; skip review when false). Hint: `Navigation.Navigate('/mercury/review/' + job.id)`.
3. Call `setReviewHandler(async (job, r) => { const { assets } = await api.resolveArt(job.appid, r.art); ...addToSteam with r.name and assets })`. `addToSteam` in `steam.ts` currently fetches art itself via `applyArt`; give it optional `name` and `assets` parameters.
4. Add the SteamGridDB key field and the "Review before adding" toggle to the Odin settings (see `pc/src/pages/Settings.tsx` for the PC version). Set the key on the Odin through the same secure route as the Real-Debrid key (scripts/set-rd-key.sh).
5. The `ready` job must stay open until the user confirms: on Unix the engine does not change state, so use `job.state === "ready"` plus a "reviewed" flag in the plugin, or add the same `review` state to the Unix flow in `jobs.rs` (line ~206 and ~545).
6. Merge `pc` into `main` once the Odin side is checked.

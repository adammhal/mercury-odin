import { api, Job } from "./api";

/** What the Review screen hands back: the title and, per art slot (0 cover, 1 hero, 2 logo, 3 wide), a SteamGridDB URL. */
export type ReviewResult = { name: string; art: Record<string, string> };

/** The PC engine adds the game itself when told to confirm. The Odin plugin replaces this to call addToSteam with
 * `api.resolveArt(job.appid, result.art)` and `result.name`, because there Steam's client is only reachable from the plugin. */
let handler: (job: Job, r: ReviewResult) => Promise<void> = async (job, r) => { await api.act(job.id, "confirm", r); };
export const setReviewHandler = (h: typeof handler) => { handler = h; };
export const confirmReview = (job: Job, r: ReviewResult) => handler(job, r);

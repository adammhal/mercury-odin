// Stand-ins for @decky/api on Windows.
import { toast } from "./ui";

export const toaster = { toast: (t: { title?: string; body: string }) => toast(t.body, t.title) };
export const fetchNoCors = (input: string, init?: RequestInit) => fetch(input, init);
export const callable = (_route: string) => async () => { throw new Error("not available on PC"); };
export const useQuickAccessVisible = () => false;
export const definePlugin = (f: unknown) => f;
export const routerHook = { addRoute() {}, removeRoute() {} };

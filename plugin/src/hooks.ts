import { useEffect, useRef, useState } from "react";

/** Re-runs `load` every `ms` while mounted. Keeps the last good value on error. */
export function usePoll<T>(load: () => Promise<T>, ms: number, deps: unknown[] = []): [T | undefined, string | undefined, () => void] {
  const [data, setData] = useState<T>();
  const [err, setErr] = useState<string>();
  const tick = useRef(0);
  const run = () => {
    const mine = ++tick.current;
    load().then((d) => { if (mine === tick.current) { setData(d); setErr(undefined); } }, (e) => setErr(String(e?.message ?? e)));
  };
  useEffect(() => {
    run();
    if (!ms) return;
    const t = setInterval(run, ms);
    return () => clearInterval(t);
  }, deps);
  return [data, err, run];
}

export function useOnce<T>(load: () => Promise<T>, deps: unknown[] = []) {
  return usePoll(load, 0, deps);
}

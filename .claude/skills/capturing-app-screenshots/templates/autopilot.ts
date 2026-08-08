/**
 * TEMPORARY dev-only remote-control hook used to drive the app for
 * documentation screenshots. Polls the local control server for commands
 * and evaluates them inside the webview. Copy to src/lib/autopilot.ts,
 * add the dev-gated import to src/main.tsx, and DELETE BOTH after use.
 */

const CONTROL = "http://127.0.0.1:8799";
const LAST_ID_KEY = "autopilot.last-id";

function report(id: number, ok: boolean, msg: string): void {
  const q = `id=${id}&ok=${ok ? 1 : 0}&msg=${encodeURIComponent(msg).slice(0, 3000)}`;
  void fetch(`${CONTROL}/ap?${q}`).catch(() => {});
}

async function poll(): Promise<void> {
  try {
    const res = await fetch(`${CONTROL}/cmd`, { cache: "no-store" });
    if (!res.ok) return;
    const cmd = (await res.json()) as { id?: number; code?: string };
    const lastId = Number(sessionStorage.getItem(LAST_ID_KEY) ?? "0");
    if (typeof cmd.id !== "number" || cmd.id <= lastId || !cmd.code) return;
    sessionStorage.setItem(LAST_ID_KEY, String(cmd.id));
    try {
      const fn = new Function(`return (async () => { ${cmd.code} })()`);
      const out: unknown = await fn();
      report(cmd.id, true, out === undefined ? "" : String(out));
    } catch (e) {
      report(cmd.id, false, String(e));
    }
  } catch {
    // control server not running; stay silent
  }
}

if (import.meta.env.DEV) {
  window.setInterval(() => void poll(), 400);
}

export {};

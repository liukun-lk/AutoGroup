---
name: capturing-app-screenshots
description: Use when needing screenshots of the running AutoGroup app (for the user guide, README, GitHub Pages, release notes) or when driving the real Tauri UI programmatically — clicking buttons, filling the configure form, walking upload → configure → compute → results with the real Rust backend. Also use when window screencapture fails with "could not create image from window" (macOS screen recording permission), or when app state mysteriously resets to the upload page during automation (Vite full reload). 需要给 AutoGroup 截图、更新使用指南截图、自动化驱动真实 app 时使用。
---

# Capturing AutoGroup Screenshots

## Overview

Drive the real running app from inside its own webview and render pages to PNG with `html-to-image` — no macOS screen-recording or accessibility permission needed, real Rust backend, full-page captures without window chrome.

The mechanism: a **temporary dev-only autopilot module** polls a local control server (port 8799) for JS commands and evaluates them in the webview. You send commands with `scripts/send.py`, screenshots come back via HTTP POST to the server.

`screencapture -l <windowid>` fails with `could not create image from window` unless the host terminal has Screen Recording permission — that is why this DOM-capture route exists. Don't burn time re-trying window capture.

## Setup

```bash
AP_DIR=/tmp/autogroup-autopilot          # state lives OUTSIDE the repo (see pitfall 1)
SKILL=.claude/skills/capturing-app-screenshots

cp "$SKILL/templates/autopilot.ts" src/lib/autopilot.ts
# add to src/main.tsx after the css import:
#   if (import.meta.env.DEV) {
#     void import("./lib/autopilot");
#   }

python3 "$SKILL/scripts/server.py" &     # control server on 127.0.0.1:8799
bun add -d html-to-image                 # temp dep, removed at cleanup
bun run tauri dev                        # run in background, wait for the window
```

Order is load-bearing: finish ALL repo edits (autopilot.ts, main.tsx, `bun add`)
BEFORE starting dev — each of them triggers a Vite full reload if dev is already
running. Probe readiness by sending `return document.title;` in a retry loop
(first Rust build can take minutes).

Once the app answers, pre-warm the capture library while the app still has no
state to lose (see pitfall 2 — the first import reloads the page):

```bash
python3 "$SKILL/scripts/send.py" 30 <<'JS'
const h2i = await import('/@id/html-to-image');
return typeof h2i.toPng;
JS
# TIMEOUT on the first try is the dep pre-bundling reload; re-send once
```

Demo data: copy `src-tauri/tests/fixtures/e2e_input.xlsx` (anonymized DEMO ids, 9 animals) to a display-friendly path outside the repo, e.g. `/tmp/动物实验数据示例.xlsx`.

## Driving the app

Every command is JS run inside the webview via `send.py` (arg = timeout seconds):

```bash
python3 "$SKILL/scripts/send.py" 15 <<'JS'
return document.title;
JS
```

Building blocks proven to work:

```js
// click a button by visible text
const b = [...document.querySelectorAll('button')]
  .find(x => x.textContent.replace(/\s+/g, '').includes('开始计算'));
b.click();

// set a React-controlled input (plain .value does nothing)
const setInput = (el, value) => {
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, String(value));
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
};

// import the demo file without the native dialog: seed the recent-imports
// list (localStorage key "autogroup.recent-imports"), reload, click the entry
localStorage.setItem('autogroup.recent-imports', JSON.stringify([
  { path: '/tmp/动物实验数据示例.xlsx', name: '动物实验数据示例.xlsx', importedAt: Date.now() }
]));
location.reload();  // send.py will report TIMEOUT - expected, page died

// wait for an async transition before returning
for (let i = 0; i < 100; i++) {
  await new Promise(r => setTimeout(r, 100));
  if (document.body.innerText.includes('应用场景')) break;
}
```

## Capturing

```js
const h2i = await import('/@id/html-to-image');   // vite resolves the bare specifier

// REQUIRED before any capture that shows radios/checkboxes (pitfall 3)
for (const el of document.querySelectorAll('input')) {
  if (el.type === 'radio' || el.type === 'checkbox') {
    if (el.checked) el.setAttribute('checked', ''); else el.removeAttribute('checked');
  } else {
    el.setAttribute('value', el.value);
  }
}

// full page (captures entire scroll height, not just viewport)
const url = await h2i.toPng(document.body,
  { pixelRatio: 2, backgroundColor: getComputedStyle(document.body).backgroundColor });
await fetch('http://127.0.0.1:8799/shot?name=03-configure', { method: 'POST', body: url });

// single card instead of the page: pass that element to toPng
// (find it by walking up from its title text to the rounded-border ancestor)
```

PNGs land in `$AP_DIR/shots/`. Shot names must be ASCII (`[a-zA-Z0-9_-]`) —
the server strips everything else, so a Chinese-only name collapses to nothing. Long tables: capture the whole card, then crop
`magick shot.png -crop x1500+0+0 +repage out.png` and composite a bottom fade
(`-size ${W}x120 gradient:none-white -gravity south -composite`).

If the header logo renders blank, inline `<img>` sources first (fetch → blob →
FileReader data URL → assign back to `img.src`).

## Pitfalls (each one cost real time — do not rediscover)

| # | Symptom | Cause / fix |
|---|---------|-------------|
| 1 | App state resets to upload page mid-flow | Writing ANY file under the project root (docs/, public/, .claude/) triggers a Vite full reload. Keep all state in `$AP_DIR`; copy screenshots into the repo only after all captures are done |
| 2 | First `import('/@id/html-to-image')` command times out | Vite dep pre-bundling reloads the page, killing the in-flight command. Just re-send once |
| 3 | Screenshot shows the wrong radio selected / checkbox unchecked | `checked` is a DOM property; DOM cloning only copies attributes. Run the property→attribute sync above right before capture |
| 4 | Autopilot silently ignores a command | Hand-written cmd.json with a multi-line string is invalid JSON. Always go through `send.py` |
| 5 | `could not create image from window` | macOS Screen Recording permission missing for the shell's host app. Expected; this skill's DOM route avoids it entirely |
| 6 | send.py reports TIMEOUT after `location.reload()` | Normal: the page died before reporting. sessionStorage keeps the command from re-running after reload |

## Cleanup (mandatory — the autopilot is an eval endpoint)

```bash
rm src/lib/autopilot.ts                  # and revert the import in src/main.tsx
bun remove html-to-image
pkill -f "capturing-app-screenshots/scripts/server.py"
# stop the dev instance via its background-task PID if you have it; otherwise:
pkill -f "target/debug/autogroup"        # never bare `pkill -f vite` (too broad)
git status --short                       # verify only intended files remain
```

Never commit `autopilot.ts`, the `main.tsx` import, or the `html-to-image` dependency.

import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(here, "..");
const repoRoot = path.resolve(appRoot, "../..");
const browser = process.env.CHATLINK_BROWSER
  ?? "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = await mkdtemp(path.join(tmpdir(), "chatlink-idb-"));
const port = 5175;
const previewPort = 4175;
const vite = spawn(process.execPath, [
  path.join(repoRoot, "node_modules", "vite", "bin", "vite.js"),
  "--host", "127.0.0.1", "--port", String(port), "--strictPort",
], { cwd: appRoot, windowsHide: true, stdio: "ignore" });
const preview = spawn(process.execPath, [
  path.join(repoRoot, "node_modules", "vite", "bin", "vite.js"),
  "preview", "--host", "127.0.0.1", "--port", String(previewPort), "--strictPort",
], { cwd: appRoot, windowsHide: true, stdio: "ignore" });
let chrome;
let socket;

async function waitForServer(url) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch { /* server still starting */ }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Vite test server did not start");
}

async function waitForValue(expression, predicate, label) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const response = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    const value = response.result?.value;
    if (predicate(value)) return value;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for ${label}`);
}

async function openDevTools() {
  chrome = spawn(browser, [
    "--headless=new", "--no-first-run", "--no-default-browser-check", "--no-sandbox",
    "--disable-gpu", "--disable-gpu-compositing", "--disable-software-rasterizer",
    "--remote-debugging-port=0", "--remote-allow-origins=*",
    `--user-data-dir=${profile}`, "about:blank",
  ], { windowsHide: true, stdio: "ignore" });
  let cdpPort;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      cdpPort = Number((await readFile(path.join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0]);
      if (cdpPort > 0) break;
    } catch { /* browser still starting */ }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!cdpPort) throw new Error("Chrome DevTools did not start");
  const targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
  const target = targets.find((item) => item.type === "page");
  if (!target) throw new Error("No browser page target");
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
}

let nextId = 1;
const pending = new Map();
function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function runPhase(phase) {
  await send("Page.navigate", { url: `http://127.0.0.1:${port}/tests/storage.html?phase=${phase}` });
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const response = await send("Runtime.evaluate", {
      expression: "({ result: document.body?.dataset.result, text: document.body?.textContent })",
      returnByValue: true,
    });
    const value = response.result?.value;
    if (value?.result === "PASS") return;
    if (value?.result === "FAIL") throw new Error(`IndexedDB ${phase} failed: ${value.text}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`IndexedDB ${phase} timed out`);
}

async function checkMobileViewport(width, height) {
  await send("Emulation.setDeviceMetricsOverride", {
    width, height, deviceScaleFactor: 1, mobile: true,
  });
  await send("Page.navigate", { url: `http://127.0.0.1:${port}/` });
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const response = await send("Runtime.evaluate", {
      expression: "({ ready: !!document.querySelector('.setup-card'), width: innerWidth, scrollWidth: document.documentElement.scrollWidth })",
      returnByValue: true,
    });
    const value = response.result?.value;
    if (value?.ready) {
      if (value.scrollWidth > value.width) {
        throw new Error(`Mobile layout overflows at ${width}px: ${value.scrollWidth}px`);
      }
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Mobile UI did not render at ${width}px`);
}

async function checkOfflinePwaShell() {
  await send("Emulation.clearDeviceMetricsOverride");
  await send("Page.navigate", { url: `http://127.0.0.1:${previewPort}/` });
  await waitForValue(
    "({ ready: !!document.querySelector('.setup-card'), sw: !!navigator.serviceWorker?.controller })",
    (value) => value?.ready, "production PWA shell",
  );
  await waitForValue(
    "!!navigator.serviceWorker?.getRegistrations && navigator.serviceWorker.getRegistrations().then(r => r.length > 0)",
    (value) => value === true, "Service Worker registration",
  );
  await send("Page.reload");
  await waitForValue(
    "!!navigator.serviceWorker?.controller",
    (value) => value === true, "Service Worker control",
  );
  await send("Network.enable");
  await send("Network.emulateNetworkConditions", {
    offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0,
  });
  await send("Page.reload");
  await waitForValue(
    "({ shell: !!document.querySelector('.setup-card'), offline: !navigator.onLine })",
    (value) => value?.shell && value?.offline, "offline PWA shell",
  );
  await send("Network.emulateNetworkConditions", {
    offline: false, latency: 0, downloadThroughput: 0, uploadThroughput: 0,
  });
}

try {
  await waitForServer(`http://127.0.0.1:${port}/tests/storage.html`);
  await waitForServer(`http://127.0.0.1:${previewPort}/`);
  await openDevTools();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const waiting = pending.get(message.id);
    if (waiting) {
      pending.delete(message.id);
      if (message.error) waiting.reject(new Error(message.error.message));
      else waiting.resolve(message.result);
    }
  });
  await runPhase("write");
  await runPhase("read");
  await checkMobileViewport(320, 700);
  await checkMobileViewport(430, 932);
  await checkOfflinePwaShell();
  console.log("PASS: IndexedDB reload, ACK, sync and import; 320px/430px layout; production Service Worker offline shell");
} finally {
  socket?.close();
  chrome?.kill();
  vite.kill();
  preview.kill();
  if (profile.startsWith(tmpdir()) && path.basename(profile).startsWith("chatlink-idb-")) {
    await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
}

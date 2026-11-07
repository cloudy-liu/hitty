// Test-only: start the app with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS set to
// --remote-debugging-port=49791, then run this against that loopback endpoint.
// The production app never enables a remote debugging port.
import { readFile, writeFile } from 'node:fs/promises';

const [endpoint, prefix] = process.argv.slice(2);
if (!endpoint || !prefix || !/^http:\/\/127\.0\.0\.1:\d+$/.test(endpoint)) {
  throw new Error('Usage: node html-preview-cdp.mjs http://127.0.0.1:PORT OUTPUT_PREFIX');
}
const pages = await (await fetch(`${endpoint}/json/list`)).json();
const page = pages.find(page => page.type === 'page' && page.url.endsWith('/tests/fixtures/html-preview/report.html'));
if (!page) throw new Error('Open the HTML acceptance fixture in the application first');
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, { once: true });
  socket.addEventListener('error', reject, { once: true });
});
let sequence = 0;
const pending = new Map();
socket.addEventListener('message', event => {
  const message = JSON.parse(event.data);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  clearTimeout(request.timeout);
  if (message.error) request.reject(new Error(JSON.stringify(message.error)));
  else request.resolve(message.result);
});
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method} timed out`));
    }, 10000);
    pending.set(id, { resolve, reject, timeout });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
try {
  const source = await readFile(new URL('html-preview-metrics.js', import.meta.url), 'utf8');
  const metrics = await send('Runtime.evaluate', { expression: `(${source})()`, returnByValue: true });
  if (metrics.exceptionDetails) throw new Error(JSON.stringify(metrics.exceptionDetails));
  await writeFile(`${prefix}.json`, JSON.stringify(metrics.result.value, null, 2) + '\n');
  const screenshot = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  await writeFile(`${prefix}.png`, Buffer.from(screenshot.data, 'base64'));
  console.log(JSON.stringify(metrics.result.value.viewport));
} finally {
  socket.close();
}

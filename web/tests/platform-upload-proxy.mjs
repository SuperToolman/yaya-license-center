// Real Next route + isolated upstream: exercises the old 512 MiB and 30s limits.
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { Readable } from 'node:stream';

let received = 0;
let forwarded;
let delay = false;
const upstream = http.createServer(async (req, res) => {
  forwarded = req.headers;
  received = 0;
  for await (const chunk of req) received += chunk.length;
  if (delay) await new Promise(resolve => setTimeout(resolve, 31000));
  res.setHeader('content-type', 'application/json');
  res.end(JSON.stringify({ message: 'ok', data: { bytes: received } }));
});
upstream.listen(0, '127.0.0.1');
await once(upstream, 'listening');
const port = upstream.address().port;
const webPort = Number(process.env.UPLOAD_TEST_WEB_PORT || 8781);
const server = spawn(process.execPath, ['node_modules/next/dist/bin/next', 'dev', '--port', String(webPort)], {
  env: { ...process.env, NEXT_DIST_DIR: '.next-upload-check', YAYA_OPERATION_API_UPSTREAM: `http://127.0.0.1:${port}` },
  stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
});
let logs = '';
for (const stream of [server.stdout, server.stderr]) stream.on('data', chunk => { logs = (logs + chunk).slice(-12000); });
const endpoint = `http://127.0.0.1:${webPort}/api/platform-package-upload`;
async function send(bytes) {
  const chunk = Buffer.alloc(1024 * 1024, 65);
  async function* body() { let left = bytes; while (left) { const count = Math.min(left, chunk.length); yield chunk.subarray(0, count); left -= count; } }
  return fetch(endpoint, { method: 'POST', duplex: 'half', body: Readable.from(body()), headers: {
    'content-type': 'multipart/form-data; boundary=test', 'content-length': String(bytes), cookie: 'test-cookie=example', origin: 'http://test.example',
  } });
}
try {
  let ready = false;
  for (let index = 0; index < 120; index++) {
    if (server.exitCode !== null) throw new Error(logs);
    try { const response = await fetch(endpoint); if (response.status === 405) { ready = true; break; } } catch {}
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  assert.ok(ready, logs);
  const bytes = 513 * 1024 * 1024;
  const response = await send(bytes);
  assert.equal(response.status, 200);
  assert.equal((await response.json()).data.bytes, bytes);
  assert.equal(forwarded.cookie, 'test-cookie=example');
  assert.equal(forwarded.origin, 'http://test.example');
  delay = true;
  const started = Date.now();
  const slow = await send(1024);
  assert.equal(slow.status, 200);
  assert.ok(Date.now() - started >= 31000);
  const tooLarge = await fetch(endpoint, { method: 'POST', headers: { 'content-type': 'multipart/form-data; boundary=x', 'content-length': String(9 * 1024 ** 3) }, body: new ReadableStream({ start(controller) { controller.close(); } }), duplex: 'half' }).catch(() => null);
  // A host fetch implementation may reject mismatched content-length locally.
  if (tooLarge) assert.equal(tooLarge.status, 413);
  console.log(JSON.stringify({ status: 'passed', forwardedBytes: bytes, validationResponseDelaySeconds: 31, productionDataChanged: false }));
} catch (error) { console.error(logs); throw error; }
finally {
  if (process.platform === 'win32') {
    const stop = spawn('taskkill.exe', ['/pid', String(server.pid), '/t', '/f'], { windowsHide: true, stdio: 'ignore' });
    await once(stop, 'exit');
  } else server.kill('SIGTERM');
  upstream.closeAllConnections(); upstream.close();
}

#!/usr/bin/env node
// PTT fan-out benchmark: one speaker + (clients-1) listeners in one group. Prints one JSON object on stdout.
// Frame layout (same as the Android client): [0x10][u16 seq BE][i32 offsetMs BE][payload]; payload starts with the
// sender's process.hrtime.bigint() (u64 BE) so listeners compute one-way latency. Listeners live in worker threads
// so the load generator's own event loop does not inflate the numbers; hrtime is shared across threads.
// usage: node ptt-bench.mjs --url ws://127.0.0.1:3100 --tokens /tmp/kw-bench-tokens.json --clients 50 --seconds 28
import { readFileSync } from 'node:fs';
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';

const HEADER = 7;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function openSocket(url, token) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${url}?token=${encodeURIComponent(token)}`);
    ws.binaryType = 'arraybuffer';
    const timer = setTimeout(() => reject(new Error('connect timeout')), 15_000);
    ws.addEventListener('open', () => { clearTimeout(timer); resolve(ws); }, { once: true });
    ws.addEventListener('error', () => { clearTimeout(timer); reject(new Error('ws error')); }, { once: true });
  });
}

if (!isMainThread) {
  const { url, tokens } = workerData;
  const latencies = []; // ms
  const seen = new Map(); // seq -> receive count
  let received = 0;
  const sockets = [];
  let connectErrors = 0;
  for (let i = 0; i < tokens.length; i += 25) {
    const batch = await Promise.allSettled(tokens.slice(i, i + 25).map((t) => openSocket(url, t)));
    for (const r of batch) {
      if (r.status === 'rejected') { connectErrors++; continue; }
      const ws = r.value;
      ws.addEventListener('message', (ev) => {
        if (!(ev.data instanceof ArrayBuffer) || ev.data.byteLength < HEADER + 8) return;
        const now = process.hrtime.bigint();
        const dv = new DataView(ev.data);
        const sent = dv.getBigUint64(HEADER);
        latencies.push(Number(now - sent) / 1e6);
        const seq = dv.getUint16(1);
        seen.set(seq, (seen.get(seq) ?? 0) + 1);
        received++;
      });
      sockets.push(ws);
    }
  }
  parentPort.postMessage({ type: 'ready', connected: sockets.length, connectErrors });
  parentPort.once('message', () => {
    const open = sockets.filter((s) => s.readyState === WebSocket.OPEN).length;
    for (const s of sockets) s.close();
    parentPort.postMessage({ type: 'done', received, open, latencies: Float64Array.from(latencies) });
  });
} else {
  const arg = (k, d) => (process.argv.includes(`--${k}`) ? process.argv[process.argv.indexOf(`--${k}`) + 1] : d);
  const url = arg('url', 'ws://127.0.0.1:3100').replace(/\/$/, '');
  const clients = Number(arg('clients', 50));
  const seconds = Number(arg('seconds', 28));
  const fps = Number(arg('fps', 50));
  const payloadBytes = Number(arg('payload', 60));
  const group = arg('group', 'bench-g1');
  const { ptt } = JSON.parse(readFileSync(arg('tokens', '/tmp/kw-bench-tokens.json'), 'utf8'));
  if (clients < 2 || clients > ptt.length) throw new Error(`--clients must be 2..${ptt.length}`);
  const wsUrl = `${url}/ws/ptt/${group}`;

  const listenerTokens = ptt.slice(1, clients);
  const nWorkers = Math.min(4, Math.ceil(listenerTokens.length / 32));
  const slices = Array.from({ length: nWorkers }, (_, i) => listenerTokens.filter((_, j) => j % nWorkers === i));
  const tConnect = performance.now();
  const workers = slices.map((tokens) => {
    const w = new Worker(new URL(import.meta.url), { workerData: { url: wsUrl, tokens } });
    const ready = new Promise((res) => w.once('message', res));
    const done = new Promise((res) => w.on('message', (m) => m.type === 'done' && res(m)));
    return { w, ready, done };
  });
  const speaker = await openSocket(wsUrl, ptt[0]);
  const readies = await Promise.all(workers.map((x) => x.ready));
  const connectMs = performance.now() - tConnect;
  const connected = 1 + readies.reduce((a, r) => a + r.connected, 0);
  const connectErrors = readies.reduce((a, r) => a + r.connectErrors, 0);
  await sleep(500);

  // Floor request -> grant latency.
  const granted = new Promise((res) => {
    const t0 = performance.now();
    speaker.addEventListener('message', function onMsg(ev) {
      if (typeof ev.data === 'string' && ev.data.includes('FLOOR_GRANTED')) {
        speaker.removeEventListener('message', onMsg);
        res(performance.now() - t0);
      }
    });
    speaker.send(JSON.stringify({ type: 'FLOOR_REQUEST', requestId: 'bench-1' }));
  });
  const floorGrantMs = await Promise.race([granted, sleep(5000).then(() => null)]);
  if (floorGrantMs === null) throw new Error('FLOOR_GRANTED not received');

  const interval = 1000 / fps;
  const total = Math.floor(seconds * fps);
  const filler = new Uint8Array(payloadBytes - 8);
  const frame = new Uint8Array(HEADER + payloadBytes);
  const dv = new DataView(frame.buffer);
  frame[0] = 0x10;
  frame.set(filler, HEADER + 8);
  const startedAt = performance.now();
  let sent = 0;
  let late = 0;
  await new Promise((resolve) => {
    const tick = () => {
      const due = Math.min(total, Math.floor((performance.now() - startedAt) / interval) + 1);
      if (due - sent > 1) late++;
      while (sent < due) {
        dv.setUint16(1, sent & 0xffff);
        dv.setInt32(3, Math.round(sent * interval));
        dv.setBigUint64(HEADER, process.hrtime.bigint());
        speaker.send(frame);
        sent++;
      }
      if (sent >= total) return resolve();
      setTimeout(tick, Math.max(0, startedAt + sent * interval - performance.now()));
    };
    tick();
  });
  speaker.send(JSON.stringify({ type: 'FLOOR_RELEASED' }));
  await sleep(1500); // let in-flight frames land

  const results = await Promise.all(workers.map((x) => { x.w.postMessage('stop'); return x.done; }));
  speaker.close();
  const lat = Float64Array.from(results.flatMap((r) => Array.from(r.latencies))).sort();
  const q = (p) => (lat.length ? lat[Math.min(lat.length - 1, Math.floor(p * lat.length))] : null);
  const listeners = connected - 1;
  const expected = listeners * sent;
  const received = results.reduce((a, r) => a + r.received, 0);
  const round = (x) => (x === null ? null : Math.round(x * 1000) / 1000);
  console.log(JSON.stringify({
    clients, connected, connectErrors, connectMs: Math.round(connectMs), floorGrantMs: round(floorGrantMs),
    framesSent: sent, senderLateTicks: late, expected, received, lost: expected - received,
    lossPct: expected ? round(((expected - received) / expected) * 100) : null,
    deliveredPerSec: Math.round(received / seconds),
    latencyMs: { p50: round(q(0.5)), p90: round(q(0.9)), p99: round(q(0.99)), max: round(lat.at(-1) ?? null) },
  }));
  process.exit(0);
}

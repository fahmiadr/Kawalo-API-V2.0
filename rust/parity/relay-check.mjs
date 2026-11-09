// Two server processes on ONE Redis: PTT frames must cross instances identically for every pairing.
//   node relay-check.mjs --left http://127.0.0.1:3410 --right http://127.0.0.1:3412 [--tokens .cache/tokens.json]
// Prints the transcript as JSON; run it for ts+ts, rs+rs, ts+rs and rs+ts and diff the outputs
// (services.sh relay does exactly that).
import { readFileSync } from 'node:fs';
import { wsHandshake } from './lib/ws.mjs';

const arg = (n, d) => {
  const i = process.argv.indexOf(`--${n}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const left = arg('left');
const right = arg('right');
const tokens = JSON.parse(readFileSync(arg('tokens', new URL('.cache/tokens.json', import.meta.url)), 'utf8'));
const tokenMap = tokens.tokens ?? tokens;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = [];

const audio = (n, len = 40) => {
  const b = Buffer.alloc(7 + len);
  b.writeUInt8(0x10, 0);
  b.writeUInt32BE(n, 1);
  b.writeUInt16BE(len, 5);
  for (let i = 0; i < len; i++) b[7 + i] = (n * 31 + i) & 0xff;
  return b;
};

const scrub = (text) => {
  try {
    const o = JSON.parse(text);
    for (const k of Object.keys(o)) if (/time|ts|at$/i.test(k)) o[k] = '<t>';
    return o;
  } catch {
    return text;
  }
};

async function open(name, base, tokenName) {
  const url = `${base.replace('http', 'ws')}/ws/ptt/grp-alpha?token=${encodeURIComponent(tokenMap[tokenName])}`;
  const hs = await wsHandshake(url);
  if (!hs.client) throw new Error(`${name}: handshake ${hs.status} ${hs.body}`);
  const c = hs.client;
  return {
    send: (o) => c.sendJson(o),
    sendBinary: (b) => c.sendBinary(b),
    async expect(label, ms = 3000) {
      const m = await c.next(ms);
      log.push({ name, label, ...(m.kind === 'text' ? { text: scrub(m.text) } : m.kind === 'binary' ? { binary: m.hex, length: m.length } : { [m.kind]: true }) });
    },
    async silence(label, ms = 500) {
      const m = await c.next(ms);
      log.push({ name, label, silence: m.kind === 'timeout' ? true : m });
    },
    async destroy() {
      try { c.close(1000); } catch { /* gone */ }
      await Promise.race([c.closed, sleep(600)]);
      c.destroy();
    },
  };
}

const opened = [];
const track = async (p) => (opened.push(await p), opened.at(-1));
try {
  const g1 = await track(open('g1@left', left, 'ptt_guard1'));
  await g1.expect('g1 initial state');
  const g2 = await track(open('g2@right', right, 'ptt_guard2'));
  await g2.expect('g2 initial state');
  await g1.expect('g1 sees member count 2');
  const a1 = await track(open('a1@right', right, 'ptt_admin1'));
  await a1.expect('a1 initial state');
  await g1.expect('g1 sees member count 3');
  await g2.expect('g2 sees member count 3');

  g1.send({ type: 'FLOOR_REQUEST', requestId: 'r1' });
  await g1.expect('g1 granted');
  await g1.expect('g1 transmitting');
  await g2.expect('g2 transmitting (remote)');
  await a1.expect('a1 transmitting (remote)');
  g2.send({ type: 'FLOOR_REQUEST', requestId: 'r2' });
  await g2.expect('g2 denied (remote request, remote holder)');
  g1.sendBinary(audio(1));
  await g2.expect('g2 audio (remote)');
  await a1.expect('a1 audio (remote)');
  await g1.silence('no echo');
  await g2.silence('exactly once on the remote instance');
  g2.sendBinary(audio(2));
  await g1.silence('audio from a non-speaker on the other instance is dropped');
  a1.send({ type: 'FLOOR_REQUEST', requestId: 'r3' });
  await a1.expect('a1 preempts across instances');
  await g1.expect('g1 revoked');
  await g1.expect('g1 reset');
  await g2.expect('g2 reset');
  await a1.expect('a1 state');
  await g1.expect('g1 state');
  await g2.expect('g2 state');
  g1.sendBinary(audio(3));
  await a1.silence('old speaker no longer relays');
  a1.sendBinary(audio(4));
  await g1.expect('g1 hears the admin (remote)');
  await g2.expect('g2 hears the admin');
  a1.send({ type: 'FLOOR_RELEASED' });
  await a1.expect('a1 released state');
  await g1.expect('g1 idle');
  await g2.expect('g2 idle');
  await g2.destroy();
  await g1.expect('g1 member count after g2 leaves');
} finally {
  await Promise.all(opened.map((o) => o.destroy()));
}
console.log(JSON.stringify(log, null, 1));

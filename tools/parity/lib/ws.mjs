// Minimal RFC 6455 client over node:http upgrade. Written by hand (instead of the global WebSocket)
// because the parity checks need to send arbitrary handshake headers (Authorization,
// Sec-WebSocket-Protocol) and to read the status/body of a rejected handshake.
import { createHash, randomBytes } from 'node:crypto';
import http from 'node:http';

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
const FIXED_KEY = Buffer.from('parity-harness-1').toString('base64'); // deterministic Sec-WebSocket-Accept

export function wsHandshake(url, { headers = {} } = {}) {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    const req = http.request({
      host: u.hostname,
      port: u.port,
      path: u.pathname + u.search,
      headers: {
        Connection: 'Upgrade',
        Upgrade: 'websocket',
        'Sec-WebSocket-Version': '13',
        'Sec-WebSocket-Key': FIXED_KEY,
        ...headers,
      },
    });
    req.on('upgrade', (res, socket, head) => {
      const expected = createHash('sha1').update(FIXED_KEY + GUID).digest('base64');
      resolve({
        status: res.statusCode,
        headers: res.headers,
        acceptOk: res.headers['sec-websocket-accept'] === expected,
        client: new WsClient(socket, head),
      });
    });
    req.on('response', (res) => {
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks).toString('utf8'), client: null }));
    });
    req.on('error', reject);
    req.setTimeout(8000, () => req.destroy(new Error('ws handshake timeout')));
    req.end();
  });
}

export class WsClient {
  constructor(socket, head) {
    this.socket = socket;
    this.buf = head?.length ? Buffer.from(head) : Buffer.alloc(0);
    this.queue = [];
    this.waiters = [];
    this.closeInfo = null;
    this.closed = new Promise((r) => (this.resolveClosed = r));
    socket.on('data', (d) => {
      this.buf = Buffer.concat([this.buf, d]);
      this.#parse();
    });
    socket.on('close', () => this.#finish({ code: this.closeInfo?.code ?? 1006, reason: this.closeInfo?.reason ?? '', clean: Boolean(this.closeInfo) }));
    socket.on('error', () => {});
    if (this.buf.length) this.#parse();
  }

  #finish(info) {
    if (this.done) return;
    this.done = true;
    this.closeInfo = info;
    for (const w of this.waiters.splice(0)) w.resolve({ kind: 'close', ...info });
    this.resolveClosed(info);
  }

  #push(msg) {
    const w = this.waiters.shift();
    if (w) w.resolve(msg);
    else this.queue.push(msg);
  }

  #parse() {
    for (;;) {
      if (this.buf.length < 2) return;
      const b0 = this.buf[0];
      const b1 = this.buf[1];
      let len = b1 & 0x7f;
      let off = 2;
      if (len === 126) {
        if (this.buf.length < 4) return;
        len = this.buf.readUInt16BE(2);
        off = 4;
      } else if (len === 127) {
        if (this.buf.length < 10) return;
        len = Number(this.buf.readBigUInt64BE(2));
        off = 10;
      }
      if (this.buf.length < off + len) return;
      const payload = this.buf.subarray(off, off + len);
      this.buf = this.buf.subarray(off + len);
      const op = b0 & 0x0f;
      if (op === 1) this.#push({ kind: 'text', text: payload.toString('utf8') });
      else if (op === 2) this.#push({ kind: 'binary', hex: payload.toString('hex'), length: payload.length });
      else if (op === 8) {
        const code = payload.length >= 2 ? payload.readUInt16BE(0) : 1005;
        const reason = payload.length > 2 ? payload.subarray(2).toString('utf8') : '';
        this.closeInfo = { code, reason };
        this.#frame(8, payload.subarray(0, 2));
        this.socket.end();
      } else if (op === 9) this.#frame(10, payload);
    }
  }

  #frame(opcode, payload) {
    if (this.socket.destroyed) return;
    const mask = randomBytes(4);
    const n = payload.length;
    const header = n < 126 ? Buffer.from([0x80 | opcode, 0x80 | n]) : Buffer.concat([Buffer.from([0x80 | opcode, 0x80 | 126]), Buffer.from([n >> 8, n & 255])]);
    const masked = Buffer.from(payload);
    for (let i = 0; i < n; i++) masked[i] ^= mask[i % 4];
    this.socket.write(Buffer.concat([header, mask, masked]));
  }

  sendText(s) {
    this.#frame(1, Buffer.from(s));
  }

  sendJson(o) {
    this.sendText(JSON.stringify(o));
  }

  sendBinary(buf) {
    this.#frame(2, Buffer.from(buf));
  }

  close(code = 1000, reason = '') {
    const p = Buffer.alloc(2 + Buffer.byteLength(reason));
    p.writeUInt16BE(code, 0);
    p.write(reason, 2);
    this.#frame(8, p);
  }

  /** Next event (text / binary / close), or {kind:'timeout'}. */
  next(timeoutMs = 3000) {
    if (this.queue.length) return Promise.resolve(this.queue.shift());
    if (this.done) return Promise.resolve({ kind: 'close', ...this.closeInfo });
    return new Promise((resolve) => {
      const waiter = { resolve: (m) => (clearTimeout(timer), resolve(m)) };
      const timer = setTimeout(() => {
        this.waiters = this.waiters.filter((w) => w !== waiter);
        resolve({ kind: 'timeout' });
      }, timeoutMs);
      this.waiters.push(waiter);
    });
  }

  destroy() {
    this.socket.destroy();
  }
}

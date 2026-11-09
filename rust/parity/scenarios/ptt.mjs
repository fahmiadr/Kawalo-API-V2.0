// PTT walkie-talkie WebSocket: handshake auth/tenancy, floor control, preemption, audio fan-out, presence, timers.
const WS = '/ws/ptt';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// 7-byte header + opus-like payload; content is irrelevant to the server but must arrive byte-identical.
const audio = (n, len = 40) => {
  const b = Buffer.alloc(7 + len);
  b.writeUInt8(0x10, 0);
  b.writeUInt32BE(n, 1);
  b.writeUInt16BE(len, 5);
  for (let i = 0; i < len; i++) b[7 + i] = (n * 31 + i) & 0xff;
  return b;
};

/** Runs `body(sockets)` and always tears the sockets down, even when an assertion path throws. */
async function withSockets(fn) {
  const opened = [];
  const track = (s) => (opened.push(s), s);
  try {
    await fn(track);
  } finally {
    await Promise.all(opened.map((s) => s.destroy()));
    await sleep(100);
  }
}

export default [
  {
    name: 'ptt/http-handshake',
    order: 70,
    async run(t) {
      const hs = (label, path, opts) =>
        t.wsSession(label, async (srv, log) => {
          await withSockets(async (track) => {
            const c = track(await srv.ws('c', path, opts, log));
            if (c.open) await c.expect('first frame after open');
          });
        });
      await hs('no token -> 401', `${WS}/grp-alpha`, {});
      await hs('garbage token in query -> 401', `${WS}/grp-alpha`, { token: 'raw:garbage' });
      await hs('expired token -> 401', `${WS}/grp-alpha`, { token: 'expired' });
      await hs('bad signature -> 401', `${WS}/grp-alpha`, { token: 'bad_signature' });
      await hs('wrong audience -> 401', `${WS}/grp-alpha`, { token: 'wrong_audience' });
      await hs('stale session -> 401', `${WS}/grp-alpha`, { token: 'stale_user' });
      await hs('token for another group -> 403', `${WS}/grp-alpha`, { token: 'ptt_beta_guard' });
      await hs('same user in its own group connects', `${WS}/grp-beta`, { token: 'ptt_beta_guard' });
      await hs('token without group may join any group', `${WS}/grp-alpha`, { token: 'ptt_nogroup' });
      await hs('SUPER_ADMIN crosses groups', `${WS}/grp-gamma`, { token: 'super_admin' });
      await hs('group id is case-sensitive -> 403', `${WS}/GRP-ALPHA`, { token: 'ptt_guard1' });
      await hs('group id percent-encoded -> 403', `${WS}/grp%20alpha`, { token: 'ptt_guard1' });
      await hs('token in Authorization header', `${WS}/grp-alpha`, { token: 'ptt_guard1', via: 'header' });
      await hs('Authorization header without Bearer prefix', `${WS}/grp-alpha`, { token: 'ptt_guard1', via: 'header-raw' });
      await hs('lowercase "bearer" prefix is accepted for ws', `${WS}/grp-alpha`, { headers: { authorization: `bearer ${t.tokens.ptt_guard1}` } });
      await hs('token in Sec-WebSocket-Protocol', `${WS}/grp-alpha`, { token: 'ptt_guard1', via: 'protocol' });
      await hs('query token wins over a bad header', `${WS}/grp-alpha`, { token: 'ptt_guard1', via: 'query', headers: { authorization: 'Bearer garbage' } });
      await hs('bad query token wins over a good header', `${WS}/grp-alpha`, { token: 'raw:garbage', via: 'query', headers: { authorization: `Bearer ${t.tokens.ptt_guard1}` } });
      await hs('empty token param falls through to the header', `${WS}/grp-alpha?token=`, { token: 'ptt_guard1', via: 'header' });
      await hs('stale token in header -> 401', `${WS}/grp-alpha`, { token: 'stale_user', via: 'header' });
      await hs('empty group id -> 404', `${WS}/`, { token: 'ptt_guard1' });
      await hs('no group segment -> 404', `${WS}`, { token: 'ptt_guard1' });
      await hs('rate-limit headers on the upgrade response', `${WS}/grp-alpha`, { token: 'ptt_guard2' });
      await t.step('plain GET (no Upgrade) on the ws path', { path: `${WS}/grp-alpha`, token: 'ptt_guard1' });
      await t.step('plain GET without token on the ws path', { path: `${WS}/grp-alpha` });
      await t.step('POST on the ws path', { method: 'POST', path: `${WS}/grp-alpha`, body: {} });
    },
  },
  {
    name: 'ptt/floor-control',
    order: 71,
    async run(t) {
      await t.wsSession('three clients: grant, deny, audio fan-out, admin preemption, release', async (srv, log) => {
        await withSockets(async (track) => {
          const g1 = track(await srv.ws('g1', `${WS}/grp-alpha`, { token: 'ptt_guard1' }, log));
          await g1.expect('g1 initial state (IDLE, 1 member)');
          const g2 = track(await srv.ws('g2', `${WS}/grp-alpha`, { token: 'ptt_guard2', via: 'header' }, log));
          await g2.expect('g2 initial state (IDLE, 2 members)');
          await g1.expect('g1 sees member count 2');
          const a1 = track(await srv.ws('a1', `${WS}/grp-alpha`, { token: 'ptt_admin1', via: 'protocol' }, log));
          await a1.expect('a1 initial state (3 members)');
          await g1.expect('g1 sees member count 3');
          await g2.expect('g2 sees member count 3');

          g1.send({ type: 'FLOOR_REQUEST', requestId: 'r1' });
          await g1.expect('g1 FLOOR_GRANTED');
          await g1.expect('g1 FLOOR_STATE_CHANGED TRANSMITTING');
          await g2.expect('g2 TRANSMITTING');
          await a1.expect('a1 TRANSMITTING');

          g2.send({ type: 'FLOOR_REQUEST', requestId: 'r2' });
          await g2.expect('g2 FLOOR_DENIED CHANNEL_BUSY (guard vs guard)');
          await g1.silence('speaker is not told about the denied request');

          g1.sendBinary(audio(1));
          await g2.expect('g2 receives g1 audio');
          await a1.expect('a1 receives g1 audio');
          await g1.silence('speaker does not hear itself');
          g2.sendBinary(audio(2));
          await g1.silence('audio from a non-speaker is dropped (g1)');
          await a1.silence('audio from a non-speaker is dropped (a1)');

          g2.send({ type: 'FLOOR_RELEASED' });
          await g1.silence('release by a non-speaker is ignored');

          a1.send({ type: 'FLOOR_REQUEST', requestId: 'r3' });
          await a1.expect('a1 FLOOR_GRANTED (preempt)');
          await g1.expect('g1 FLOOR_REVOKED ADMIN_OVERRIDE');
          await g1.expect('g1 FLOOR_RESET');
          await g2.expect('g2 FLOOR_RESET');
          await a1.expect('a1 FLOOR_STATE_CHANGED (admin transmitting)');
          await g1.expect('g1 FLOOR_STATE_CHANGED (admin transmitting)');
          await g2.expect('g2 FLOOR_STATE_CHANGED (admin transmitting)');
          await a1.silence('preempting admin gets no FLOOR_RESET');

          g1.sendBinary(audio(3));
          await g2.silence('in-flight audio of the preempted guard is dropped');
          a1.sendBinary(audio(4));
          await g1.expect('g1 receives admin audio');
          await g2.expect('g2 receives admin audio');

          g1.send({ type: 'FLOOR_REQUEST', requestId: 'r4' });
          await g1.expect('g1 FLOOR_DENIED INSUFFICIENT_PRIORITY');
          a1.send({ type: 'FLOOR_REQUEST', requestId: 'r5' });
          await a1.expect('admin re-requesting while holding the floor is denied (CHANNEL_BUSY)');

          a1.send({ type: 'FLOOR_RELEASED', reason: 'done' });
          await a1.expect('a1 IDLE');
          await g1.expect('g1 IDLE');
          await g2.expect('g2 IDLE');

          g2.send({ type: 'FLOOR_REQUEST', requestId: 'r6' });
          await g2.expect('g2 granted on a free channel');
          await g2.expect('g2 TRANSMITTING');
          await g1.expect('g1 TRANSMITTING');
          await a1.expect('a1 TRANSMITTING');

          g2.close(1000);
          await g1.expect('speaker disconnect: IDLE with 2 members (g1)');
          await a1.expect('speaker disconnect: IDLE with 2 members (a1)');

          g1.close(1000);
          await a1.expect('non-speaker disconnect: member count 1');
          a1.close(1000);
        });
      });
    },
  },
  {
    name: 'ptt/frames-edge-cases',
    order: 72,
    async run(t) {
      await t.wsSession('malformed and unusual frames', async (srv, log) => {
        await withSockets(async (track) => {
          const g1 = track(await srv.ws('g1', `${WS}/grp-alpha`, { token: 'ptt_guard3' }, log));
          await g1.expect('initial state');
          const g2 = track(await srv.ws('g2', `${WS}/grp-alpha`, { token: 'ptt_guard4' }, log));
          await g2.expect('initial state (2 members)');
          await g1.expect('member count 2');

          g1.sendText('this is not json');
          await g1.silence('non-json text is ignored');
          g1.sendText('{"type":"FLOOR_REQUEST"');
          await g1.silence('truncated json is ignored');
          g1.sendText('[]');
          await g1.silence('json array is ignored');
          g1.sendText('null');
          await g1.silence('json null is ignored');
          g1.sendText('"FLOOR_REQUEST"');
          await g1.silence('json string is ignored');
          g1.send({ type: 'SOMETHING_ELSE' });
          await g1.silence('unknown type is ignored');
          g1.send({ type: 'floor_request', requestId: 'x' });
          await g1.silence('type is case-sensitive');
          g1.send({});
          await g1.silence('empty object is ignored');
          g1.sendBinary(Buffer.alloc(0));
          await g2.silence('empty binary frame from a non-speaker is dropped');

          g1.send({ type: 'FLOOR_REQUEST' });
          await g1.expect('FLOOR_REQUEST without requestId is still granted (no validation)');
          await g1.expect('TRANSMITTING');
          await g2.expect('g2 TRANSMITTING');
          g1.sendBinary(Buffer.from([1, 2, 3]));
          await g2.expect('short binary frame is forwarded as is');
          g1.sendBinary(audio(9, 3000));
          await g2.expect('large binary frame is forwarded as is');
          g1.sendText('\u0010fallback');
          await g2.expect('text frame starting with 0x10 is forwarded as binary (latin1)');
          g1.send({ type: 'FLOOR_RELEASED' });
          await g1.expect('IDLE (g1)');
          await g2.expect('IDLE (g2)');

          g2.send({ type: 'FLOOR_REQUEST', requestId: 12345 });
          await g2.expect('numeric requestId is echoed back');
          await g2.expect('TRANSMITTING (g2)');
          await g1.expect('TRANSMITTING (g1)');
          g2.send({ type: 'FLOOR_REQUEST', requestId: '' });
          await g2.expect('empty requestId while holding the floor is denied');
          g2.send({ type: 'FLOOR_RELEASED' });
          await g2.expect('IDLE (g2)');
          await g1.expect('IDLE (g1)');
          g2.send({ type: 'FLOOR_RELEASED' });
          await g1.silence('release on an idle channel is ignored');
        });
      });
    },
  },
  {
    name: 'ptt/presence',
    order: 73,
    async run(t) {
      await t.wsSession('member count is a set of user ids', async (srv, log) => {
        await withSockets(async (track) => {
          const a = track(await srv.ws('a', `${WS}/grp-alpha`, { token: 'ptt_guard5' }, log));
          await a.expect('a initial (1 member)');
          const a2 = track(await srv.ws('a2', `${WS}/grp-alpha`, { token: 'ptt_guard5', via: 'header' }, log));
          await a2.expect('same user, second socket: still 1 member');
          await a.expect('first socket is told the count');
          const b = track(await srv.ws('b', `${WS}/grp-alpha`, { token: 'ptt_guard6' }, log));
          await b.expect('b initial (2 members)');
          await a.expect('a sees 2');
          await a2.expect('a2 sees 2');
          a2.close(1000);
          await a.expect('closing one socket of a user removes the user from the set: 1 member');
          await b.expect('b sees 1');
          b.close(1000);
          await a.expect('a sees 0');
          a.close(1000);
        });
      });
      await t.wsSession('rooms are isolated per group', async (srv, log) => {
        await withSockets(async (track) => {
          const al = track(await srv.ws('alpha', `${WS}/grp-alpha`, { token: 'ptt_guard1' }, log));
          await al.expect('alpha initial');
          const be = track(await srv.ws('beta', `${WS}/grp-beta`, { token: 'ptt_beta_guard' }, log));
          await be.expect('beta initial');
          await al.silence('beta joining does not reach alpha');
          be.send({ type: 'FLOOR_REQUEST', requestId: 'b1' });
          await be.expect('beta granted');
          await be.expect('beta transmitting');
          await al.silence('beta floor does not reach alpha');
          be.sendBinary(audio(5));
          await al.silence('beta audio does not reach alpha');
          const al2 = track(await srv.ws('alpha2', `${WS}/grp-alpha`, { token: 'ptt_guard2' }, log));
          await al2.expect('alpha is still IDLE');
          be.close(1000);
          await al.expect('alpha sees its own count only');
          al.close(1000);
          al2.close(1000);
        });
      });
      await t.wsSession('joining while someone transmits shows the current speaker', async (srv, log) => {
        await withSockets(async (track) => {
          const s = track(await srv.ws('speaker', `${WS}/grp-gamma`, { token: 'super_admin' }, log));
          await s.expect('speaker initial');
          s.send({ type: 'FLOOR_REQUEST', requestId: 'g1' });
          await s.expect('granted');
          await s.expect('transmitting');
          const late = track(await srv.ws('late', `${WS}/grp-gamma`, { token: 'ptt_nogroup' }, log));
          await late.expect('late joiner sees TRANSMITTING with the speaker');
          await s.expect('speaker sees the new member count');
          late.send({ type: 'FLOOR_REQUEST', requestId: 'g2' });
          await late.expect('late joiner is denied (CHANNEL_BUSY)');
          s.close(1000);
          await late.expect('late joiner sees IDLE after the speaker leaves');
          late.close(1000);
        });
      });
    },
  },
  {
    name: 'ptt/lease-expiry',
    order: 74,
    async run(t) {
      await t.wsSession('the 5s floor lease expires without audio', async (srv, log) => {
        await withSockets(async (track) => {
          const g1 = track(await srv.ws('g1', `${WS}/grp-alpha`, { token: 'ptt_guard1' }, log));
          await g1.expect('initial');
          const g2 = track(await srv.ws('g2', `${WS}/grp-alpha`, { token: 'ptt_guard2' }, log));
          await g2.expect('initial');
          await g1.expect('count 2');
          g1.send({ type: 'FLOOR_REQUEST', requestId: 'l1' });
          await g1.expect('granted');
          await g1.expect('transmitting');
          await g2.expect('transmitting');
          g2.send({ type: 'FLOOR_REQUEST', requestId: 'l2' });
          await g2.expect('denied while the lease is live');
          await sleep(5600);
          g2.send({ type: 'FLOOR_REQUEST', requestId: 'l3' });
          await g2.expect('granted after the lease expired');
          await g2.expect('transmitting');
          await g1.expect('g1 sees g2 transmitting');
          g1.sendBinary(audio(7));
          await g2.silence('audio of the stale speaker is dropped');
        });
      });
    },
  },
  {
    name: 'ptt/audio-keeps-lease',
    order: 75,
    slow: true,
    async run(t) {
      await t.wsSession('audio renews the lease; TOT revokes after 30s', async (srv, log) => {
        await withSockets(async (track) => {
          const g1 = track(await srv.ws('g1', `${WS}/grp-alpha`, { token: 'ptt_guard1' }, log));
          await g1.expect('initial');
          const g2 = track(await srv.ws('g2', `${WS}/grp-alpha`, { token: 'ptt_guard2' }, log));
          await g2.expect('initial');
          await g1.expect('count 2');
          g1.send({ type: 'FLOOR_REQUEST', requestId: 't1' });
          await g1.expect('granted');
          await g1.expect('transmitting');
          await g2.expect('transmitting');
          for (let i = 0; i < 12; i++) {
            g1.sendBinary(audio(100 + i));
            await g2.expect(`audio ${i}`);
            await sleep(600);
          }
          g2.send({ type: 'FLOOR_REQUEST', requestId: 't2' });
          await g2.expect('still denied after 7s thanks to the renewals');
          // no more audio: the Redis lease lapses but the 30s time-out timer still fires
          await g1.expect('g1 FLOOR_REVOKED TIME_OUT_TIMER_EXPIRED', 30000);
          await g1.expect('g1 IDLE after TOT');
          await g2.expect('g2 IDLE after TOT');
        });
      });
    },
  },
];

// Incidents: list, detail, create, reply, resolve, delete, and the SSE stream (new_reply, thread_resolved, incident_deleted, sync_required, heartbeat).
import { listMatrix } from '../lib/cases.mjs';

const V1 = '/api/v1/incidents';
const I1 = 'inc_00000000-0000-0000-0000-000000000001'; // OPEN, 2 replies
const I2 = 'inc_00000000-0000-0000-0000-000000000002'; // RESOLVED
const I3 = 'inc_00000000-0000-0000-0000-000000000003'; // soft-deleted
const I4 = 'inc_00000000-0000-0000-0000-000000000004'; // junk photos / N/A description / zero coordinates
const I5 = 'inc_00000000-0000-0000-0000-000000000005'; // other group

export default [
  {
    name: 'incident/read',
    order: 60,
    async run(t) {
      await listMatrix(t, {
        path: V1,
        token: 'admin_g1',
        tenants: [
          { label: 'beta (has data)', token: 'admin_g2' },
          { label: 'gamma (no data)', token: 'admin_g3' },
          { label: 'no active group', token: 'user_nogroup' },
        ],
        searches: ['lampu', 'pagar', 'mencurigakan', 'budi', 'dewi', 'terhapus', 'blok a'],
        dates: [
          { label: 'single day (utc)', query: { start_date: '2026-03-01T00:00:00Z', end_date: '2026-03-01T23:59:59Z' } },
          { label: 'date only', query: { start_date: '2026-03-02', end_date: '2026-03-04' } },
          { label: 'with +07:00 offset', query: { start_date: '2026-03-02T15:00:00+07:00', end_date: '2026-03-05T06:00:00+07:00' } },
          { label: 'only start', query: { start_date: '2026-03-02T00:00:00Z' } },
          { label: 'only end', query: { end_date: '2026-03-02T00:00:00Z' } },
        ],
      });
      const q = (label, query, token = 'admin_g1', expect) => t.step(label, { path: V1, token, query, ...(expect ? { expect } : {}) });
      await q('status=OPEN', { status: 'OPEN' });
      await q('status=RESOLVED', { status: 'RESOLVED' });
      await q('status lowercase is upper-cased', { status: 'open' });
      await q('status padded with spaces', { status: ' resolved ' });
      await q('status unknown is ignored', { status: 'CLOSED' });
      await q('status empty is ignored', { status: '' });
      await q('status + search + paging', { status: 'OPEN', search: 'a', limit: 1, offset: 1 });
      await q('guard reads the whole group', { limit: 100 }, 'guard1_g1');
      await q('resident reads the whole group', { limit: 100 }, 'resident_g1');

      const get = (label, id, token = 'admin_g1', expect) => t.step(label, { path: `${V1}/${id}`, token, ...(expect ? { expect } : {}) });
      await get('detail with replies', I1, 'admin_g1', 200);
      await get('detail of a resolved incident', I2, 'admin_g1', 200);
      await get('detail with junk photos / description', I4, 'admin_g1', 200);
      await get('soft-deleted incident -> 404', I3, 'admin_g1', 404);
      await get('incident of another group -> 404', I5, 'admin_g1', 404);
      await get('same incident from its own group', I5, 'admin_g2', 200);
      await get('unknown id -> 404', 'inc_tidak-ada', 'admin_g1', 404);
      await get('id of 64 chars', 'a'.repeat(64), 'admin_g1', 404);
      await get('id longer than 64 -> 422', 'a'.repeat(65), 'admin_g1', 422);
      await get('id with odd characters is just not found', 'a%20b', 'admin_g1', 404);
      await get('id is case sensitive?', I1.toUpperCase(), 'admin_g1');
      await get('no active group -> 400', I1, 'user_nogroup', 400);
      await get('no token -> 401', I1, null, 401);
      await get('stale session -> 401', I1, 'stale_user', 401);
    },
  },
  {
    name: 'incident/create',
    order: 61,
    async run(t) {
      const create = (label, body, token = 'guard1_g1', extra = {}) => t.step(label, { method: 'POST', path: V1, token, body, ...extra });
      await create('minimal', { title: 'Pagar rusak baru' }, 'guard1_g1', { expect: 201, capture: { i1: 'data.id' } });
      await create('full', {
        title: 'Lengkap', description: 'Deskripsi panjang', category: 'security', glat: -6.2088, glong: 106.8456, photos: ['https://a/1.jpg', 'https://a/2.jpg'],
      }, 'guard1_g1', { expect: 201, capture: { i2: 'data.id' } });
      await create('resident may report', { title: 'Dari warga', category: 'facility' }, 'resident_g1', { expect: 201 });
      await create('category is case-insensitive', { title: 'Kategori besar', category: 'HAZARD' }, 'guard1_g1', { expect: 201 });
      for (const c of ['facility', 'security', 'hazard', 'general']) await create(`category ${c}`, { title: `Kat ${c}`, category: c }, 'guard1_g1', { expect: 201 });
      await create('category null defaults to general', { title: 'Kat null', category: null }, 'guard1_g1', { expect: 201 });
      await create('group_id in the body is ignored (tenant comes from the token)', { title: 'Grup lain', group_id: 'grp-beta' }, 'guard1_g1', { expect: 201 });
      await create('description null', { title: 'Desc null', description: null }, 'guard1_g1', { expect: 201 });
      await create('description "N/A" and whitespace are dropped', { title: 'Desc NA', description: 'N/A' }, 'guard1_g1', { expect: 201 });
      await create('title and description are trimmed', { title: '  Judul Spasi  ', description: '  isi  ' }, 'guard1_g1', { expect: 201 });
      await create('coordinates null/null', { title: 'Koordinat null', glat: null, glong: null }, 'guard1_g1', { expect: 201 });
      await create('coordinates at the limits', { title: 'Koordinat batas', glat: 90, glong: -180 }, 'guard1_g1', { expect: 201 });
      await create('coordinates zero/zero are kept', { title: 'Koordinat nol', glat: 0, glong: 0 }, 'guard1_g1', { expect: 201 });
      await create('coordinate with many decimals', { title: 'Presisi', glat: -6.123456789012345, glong: 106.987654321098765 }, 'guard1_g1', { expect: 201 });
      await create('photos blank entries are dropped', { title: 'Foto kosong', photos: ['', '  ', 'N/A', 'https://a/ok.jpg'] }, 'guard1_g1', { expect: 201 });
      await create('photos empty array', { title: 'Foto []', photos: [] }, 'guard1_g1', { expect: 201 });
      await create('creator name falls back from the token', { title: 'Admin lapor' }, 'admin_g1', { expect: 201 });
      await create('unknown fields dropped', { title: 'Ekstra', status: 'RESOLVED', created_by: 'x@y.z', id: 'inc_x' }, 'guard1_g1', { expect: 201 });
      await create('title max length 128', { title: 't'.repeat(128) }, 'guard1_g1', { expect: 201 });
      await create('description max length 1024', { title: 'Panjang', description: 'd'.repeat(1024) }, 'guard1_g1', { expect: 201 });
      await create('unicode', { title: 'Kebakaran 🔥 山田', description: 'Api di blok C' }, 'guard1_g1', { expect: 201 });

      const bad = (label, body, token = 'guard1_g1') => create(label, body, token, { expect: 422 });
      await bad('title missing', {});
      await bad('title empty', { title: '' });
      await bad('title too long', { title: 't'.repeat(129) });
      await bad('title wrong type', { title: 5 });
      await bad('description too long', { title: 'x', description: 'd'.repeat(1025) });
      await bad('category too long', { title: 'x', category: 'c'.repeat(17) });
      await bad('category wrong type', { title: 'x', category: 5 });
      await bad('glat out of range', { title: 'x', glat: 90.5, glong: 0 });
      await bad('glong out of range', { title: 'x', glat: 0, glong: -180.5 });
      await bad('glat as string', { title: 'x', glat: '1', glong: 1 });
      await bad('photos more than 4', { title: 'x', photos: ['a', 'b', 'c', 'd', 'e'] });
      await bad('photo too long', { title: 'x', photos: ['p'.repeat(2049)] });
      await bad('photos null', { title: 'x', photos: null });
      await bad('group_id wrong type', { title: 'x', group_id: 5 });
      await bad('array body', []);
      await bad('null body', null);
      await create('title whitespace only -> 400', { title: '   ' }, 'guard1_g1', { expect: 400 });
      await create('title "N/A" -> 400', { title: 'N/A' }, 'guard1_g1', { expect: 400 });
      await create('category not in the list -> 400', { title: 'x', category: 'lainnya' }, 'guard1_g1', { expect: 400 });
      await create('category whitespace -> default general', { title: 'x', category: '   ' }, 'guard1_g1', { expect: 201 });
      await create('glat without glong -> 400', { title: 'x', glat: 1 }, 'guard1_g1', { expect: 400 });
      await create('glong without glat -> 400', { title: 'x', glong: 1 }, 'guard1_g1', { expect: 400 });
      await create('glat given, glong null -> 400', { title: 'x', glat: 1, glong: null }, 'guard1_g1', { expect: 400 });
      await create('no active group -> 400', { title: 'x' }, 'user_nogroup', { expect: 400 });
      await create('no token', { title: 'x' }, null, { expect: 401 });
      await create('no token + invalid body: validation first', {}, null);
      await t.step('malformed json', { method: 'POST', path: V1, token: 'guard1_g1', rawBody: '{"title"', expect: 400 });
      await t.step('no body', { method: 'POST', path: V1, token: 'guard1_g1' });

      await t.step('detail of created incident', { path: (v) => `${V1}/${v.i1}`, token: 'resident_g1', expect: 200 });
      await t.step('detail of full incident', { path: (v) => `${V1}/${v.i2}`, token: 'admin_g1', expect: 200 });
      await t.step('detail from another tenant -> 404', { path: (v) => `${V1}/${v.i1}`, token: 'admin_g2', expect: 404 });
      await t.step('list, newest first', { path: V1, token: 'admin_g1', query: { limit: 5 } });
      await t.step('beta list unchanged', { path: V1, token: 'admin_g2', query: { limit: 100 } });
    },
  },
  {
    name: 'incident/created-at-order',
    order: 62,
    async run(t) {
      // CreatedAt has one-second resolution, so rows created in the same second tie in "ORDER BY CreatedAt DESC".
      for (let i = 1; i <= 3; i++) {
        await t.step(`create ${i}`, { method: 'POST', path: V1, token: 'guard2_g1', body: { title: `Urutan ${i}` }, expect: 201 });
        await t.sleep(1100);
      }
      await t.step('list order is by creation time, newest first', { path: V1, token: 'admin_g1', query: { search: 'Urutan' } });
    },
    slow: false,
  },
  {
    name: 'incident/reply-resolve-delete',
    order: 63,
    async run(t) {
      const mk = (label, key, token = 'guard1_g1', body = { title: label }) =>
        t.step(label, { method: 'POST', path: V1, token, body, expect: 201, capture: { [key]: 'data.id' } });
      await mk('incident a', 'a');
      await mk('incident b', 'b');
      await mk('incident c', 'c');

      const reply = (label, id, body, token = 'guard2_g1', extra = {}) =>
        t.step(label, { method: 'POST', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}/replies`, token, body, ...extra });
      await reply('guard replies', (v) => v.a, { message: 'Sedang ditangani' }, 'guard2_g1', { expect: 201 });
      await t.sleep(5);
      await reply('admin replies with photos', (v) => v.a, { message: 'Teknisi dikirim', photos: ['https://a/r1.jpg', 'https://a/r2.jpg'] }, 'admin_g1', { expect: 201 });
      await t.sleep(5);
      await reply('resident replies (role USER)', (v) => v.a, { message: 'Terima kasih' }, 'resident_g1', { expect: 201 });
      await t.sleep(5);
      await reply('photo-only reply (empty message)', (v) => v.a, { message: '', photos: ['https://a/only.jpg'] }, 'guard1_g1', { expect: 201 });
      await t.sleep(5);
      await reply('message is trimmed', (v) => v.a, { message: '   dengan spasi   ' }, 'guard1_g1', { expect: 201 });
      await t.sleep(5);
      await reply('unicode message', (v) => v.a, { message: 'Siap 👍 山田' }, 'guard1_g1', { expect: 201 });
      await reply('empty message and no photos -> 400', (v) => v.a, { message: '' }, 'guard2_g1', { expect: 400 });
      await reply('whitespace message and no photos -> 400', (v) => v.a, { message: '   ' }, 'guard2_g1', { expect: 400 });
      await reply('"N/A" message and photos [] -> 400', (v) => v.a, { message: 'N/A', photos: [] }, 'guard2_g1', { expect: 400 });
      await reply('blank photos only -> 400', (v) => v.a, { message: '', photos: ['', '  '] }, 'guard2_g1', { expect: 400 });
      await reply('message missing -> 422', (v) => v.a, {}, 'guard2_g1', { expect: 422 });
      await reply('message too long -> 422', (v) => v.a, { message: 'm'.repeat(2001) }, 'guard2_g1', { expect: 422 });
      await reply('message at max length', (v) => v.a, { message: 'm'.repeat(2000) }, 'guard2_g1', { expect: 201 });
      await reply('message wrong type -> 422', (v) => v.a, { message: 5 }, 'guard2_g1', { expect: 422 });
      await reply('too many photos -> 422', (v) => v.a, { message: 'x', photos: ['a', 'b', 'c', 'd', 'e'] }, 'guard2_g1', { expect: 422 });
      await reply('unknown incident -> 404', 'inc_tidak-ada', { message: 'x' }, 'guard2_g1', { expect: 404 });
      await reply('deleted incident -> 404', I3, { message: 'x' }, 'admin_g1', { expect: 404 });
      await reply('other group -> 404', I5, { message: 'x' }, 'admin_g1', { expect: 404 });
      await reply('id too long -> 422', 'a'.repeat(65), { message: 'x' }, 'admin_g1', { expect: 422 });
      await reply('resolved seed incident -> 400', I2, { message: 'x' }, 'admin_g1', { expect: 400 });
      await reply('no active group -> 400', I1, { message: 'x' }, 'user_nogroup', { expect: 400 });
      await reply('no token -> 401', I1, { message: 'x' }, null, { expect: 401 });
      await reply('no token + invalid body: validation first', I1, {}, null);
      await t.step('reply malformed json', { method: 'POST', path: `${V1}/${I1}/replies`, token: 'admin_g1', rawBody: '{', expect: 400 });
      await t.step('detail lists replies in creation order', { path: (v) => `${V1}/${v.a}`, token: 'admin_g1', expect: 200 });

      const res = (label, id, token, extra = {}) =>
        t.step(label, { method: 'PATCH', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}/resolve`, token, ...extra });
      await res('guard resolves', (v) => v.a, 'guard2_g1', { expect: 200 });
      await res('resolving again is idempotent (same resolver, same time)', (v) => v.a, 'admin_g1', { expect: 200 });
      await res('resident can resolve too', (v) => v.b, 'resident_g1', { expect: 200 });
      await res('resolve a seed that is already resolved', I2, 'guard1_g1', { expect: 200 });
      await res('unknown -> 404', 'inc_tidak-ada', 'admin_g1', { expect: 404 });
      await res('deleted -> 404', I3, 'admin_g1', { expect: 404 });
      await res('other group -> 404', I5, 'admin_g1', { expect: 404 });
      await res('id too long -> 422', 'a'.repeat(65), 'admin_g1', { expect: 422 });
      await res('no active group -> 400', I1, 'user_nogroup', { expect: 400 });
      await res('no token -> 401', I1, null, { expect: 401 });
      await reply('reply after resolve -> 400', (v) => v.a, { message: 'terlambat' }, 'guard2_g1', { expect: 400 });
      await t.step('detail after resolve', { path: (v) => `${V1}/${v.a}`, token: 'admin_g1', expect: 200 });
      await t.step('list status=RESOLVED includes the new ones', { path: V1, token: 'admin_g1', query: { status: 'RESOLVED' } });

      const del = (label, id, token, extra = {}) =>
        t.step(label, { method: 'DELETE', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}`, token, ...extra });
      await del('guard may not delete (no allow_admin)', (v) => v.c, 'guard1_g1', { expect: 403 });
      await del('resident may not delete', (v) => v.c, 'resident_g1', { expect: 403 });
      await del('ADMIN role without allow_admin may not delete', (v) => v.c, 'admin2_g1', { expect: 403 });
      await del('forbidden check runs before existence', 'inc_tidak-ada', 'guard1_g1', { expect: 403 });
      await del('no active group: allow_admin false -> 403 first?', I1, 'user_nogroup', {});
      await del('other tenant admin -> 404', (v) => v.c, 'admin_g2', { expect: 404 });
      await del('unknown -> 404', 'inc_tidak-ada', 'admin_g1', { expect: 404 });
      await del('id too long -> 422', 'a'.repeat(65), 'admin_g1', { expect: 422 });
      await del('no token -> 401', I1, null, { expect: 401 });
      await del('supervisor (GUARD with allow_admin) may delete', (v) => v.c, 'supervisor_g1', { expect: 200 });
      await del('deleting again -> 404', (v) => v.c, 'admin_g1', { expect: 404 });
      await t.step('detail after delete -> 404', { path: (v) => `${V1}/${v.c}`, token: 'admin_g1', expect: 404 });
      await t.step('list no longer has it', { path: V1, token: 'admin_g1', query: { search: 'incident c' } });
      await reply('reply to deleted -> 404', (v) => v.c, { message: 'x' }, 'guard1_g1', { expect: 404 });
      await res('resolve deleted -> 404', (v) => v.c, 'guard1_g1', { expect: 404 });
      await del('admin deletes a resolved incident', (v) => v.b, 'admin_g1', { expect: 200 });
    },
  },
  {
    name: 'incident/sse',
    order: 64,
    async run(t) {
      const mk = (label, key, token = 'sse_guard1') =>
        t.step(label, { method: 'POST', path: V1, token, body: { title: label }, expect: 201, capture: { [key]: 'data.id' } });
      await mk('stream incident', 'inc');
      await mk('stream incident for reconnect', 'inc2');

      // plain JSON errors before any stream is opened
      await t.sse('no token -> JSON 401', { path: `${V1}/${I1}/stream` }, async () => {});
      await t.sse('invalid id -> JSON 422', { path: `${V1}/${'a'.repeat(65)}/stream`, token: 'sse_guard1' }, async () => {});
      await t.sse('unknown incident -> JSON 404', { path: `${V1}/inc_tidak-ada/stream`, token: 'sse_guard1' }, async () => {});
      await t.sse('deleted incident -> JSON 404', { path: `${V1}/${I3}/stream`, token: 'sse_guard1' }, async () => {});
      await t.sse('other tenant -> JSON 404', { path: `${V1}/${I5}/stream`, token: 'sse_guard1' }, async () => {});
      await t.sse('no active group -> JSON 400', { path: `${V1}/${I1}/stream`, token: 'user_nogroup' }, async () => {});
      await t.sse('stale session -> JSON 401', { path: `${V1}/${I1}/stream`, token: 'stale_user' }, async () => {});

      await t.sse('full lifecycle: open, reply, reply with photo, resolve, delete', { path: (v) => `${V1}/${v.inc}/stream`, token: 'sse_guard2' }, async (s) => {
        await s.expect('retry hint');
        await s.silence('no sync_required on a first connection', 300);
        await s.req('guard 1 replies', { method: 'POST', path: `${V1}/${s.srv.vars.inc}/replies`, token: 'sse_guard1', body: { message: 'Halo dari SSE' } });
        await s.expect('new_reply');
        await s.sleep(5);
        await s.req('admin replies with photos', { method: 'POST', path: `${V1}/${s.srv.vars.inc}/replies`, token: 'sse_admin', body: { message: 'Foto', photos: ['https://a/s.jpg'] } });
        await s.expect('new_reply with photos');
        await s.req('reply to another incident does not leak into this stream', { method: 'POST', path: `${V1}/${s.srv.vars.inc2}/replies`, token: 'sse_guard1', body: { message: 'lain' } });
        await s.silence('nothing for other incidents', 400);
        await s.req('resolve', { method: 'PATCH', path: `${V1}/${s.srv.vars.inc}/resolve`, token: 'sse_guard1' });
        await s.expect('thread_resolved');
        await s.req('resolve again is idempotent: no second event', { method: 'PATCH', path: `${V1}/${s.srv.vars.inc}/resolve`, token: 'sse_admin' });
        await s.silence('no duplicate thread_resolved', 400);
        await s.req('reply after resolve is rejected: no event', { method: 'POST', path: `${V1}/${s.srv.vars.inc}/replies`, token: 'sse_guard1', body: { message: 'x' } });
        await s.silence('no event for a rejected reply', 300);
        await s.req('delete', { method: 'DELETE', path: `${V1}/${s.srv.vars.inc}`, token: 'sse_admin' });
        await s.expect('incident_deleted');
        await s.silence('nothing after delete', 300);
      });

      await t.sse('reconnect with Last-Event-ID gets sync_required', { path: (v) => `${V1}/${v.inc2}/stream`, token: 'sse_guard2', headers: { 'last-event-id': 'rpl_bukan-id' } }, async (s) => {
        await s.expect('retry hint');
        await s.expect('sync_required');
        await s.req('reply still streams after the sync marker', { method: 'POST', path: `${V1}/${s.srv.vars.inc2}/replies`, token: 'sse_guard1', body: { message: 'setelah sync' } });
        await s.expect('new_reply');
      });
      await t.sse('empty Last-Event-ID is treated as a first connection', { path: (v) => `${V1}/${v.inc2}/stream`, token: 'sse_guard2', headers: { 'last-event-id': '' } }, async (s) => {
        await s.expect('retry hint');
        await s.silence('no sync_required', 300);
      });
      await t.sse('resident may stream', { path: `${V1}/${I1}/stream`, token: 'resident_g1' }, async (s) => {
        await s.expect('retry hint');
        await s.silence('quiet', 200);
      });
      await t.sse('heartbeat comment arrives about every 15s', { path: `${V1}/${I1}/stream`, token: 'admin_g1' }, async (s) => {
        await s.expect('retry hint');
        await s.heartbeat('first ping');
      });
    },
  },
];

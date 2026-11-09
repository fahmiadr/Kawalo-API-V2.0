// Guests: master types, list/search/filter, detail, create, acknowledge, checkout, update (PUT+PATCH), delete (single + batch).
import { listMatrix } from '../lib/cases.mjs';

const V1 = '/api/v1/guests';
const base = { guest_name: 'Tamu Uji', guest_destination: 'Blok Z9', guest_type: 'Tamu Umum' };

export default [
  {
    name: 'guest/read',
    order: 30,
    async run(t) {
      const get = (label, path, token = 'admin_g1', extra = {}) => t.step(label, { path: `${V1}${path}`, token, ...extra });
      await get('master types (alpha)', '/master-types', 'admin_g1', { expect: 200 });
      await get('master types are open to any role', '/master-types', 'resident_g1', { expect: 200 });
      await get('master types (beta shares a name with alpha)', '/master-types', 'admin_g2', { expect: 200 });
      await get('master types (gamma, none)', '/master-types', 'admin_g3', { expect: 200 });
      await get('master types without active group', '/master-types', 'user_nogroup', { expect: 200 });
      await get('master types without token', '/master-types', null, { expect: 401 });
      await get('master-types is not captured by /:id', '/master-types/', 'admin_g1');

      await listMatrix(t, {
        path: V1,
        token: 'admin_g1',
        tenants: [
          { label: 'beta (has data)', token: 'admin_g2' },
          { label: 'gamma (no data)', token: 'admin_g3' },
          { label: 'no active group', token: 'user_nogroup' },
        ],
        searches: ['rudi', 'b1234', '3201010101010001', 'blok a1', 'paket', 'kurir', 'qr-a-0003', 'budi', 'dewi', 'andi', 'resident@alpha', 'melati', 'ktp', 'umum'],
        dates: [
          { label: 'single day (utc)', query: { start_date: '2026-03-01T00:00:00Z', end_date: '2026-03-01T23:59:59Z' } },
          { label: 'date only', query: { start_date: '2026-03-02', end_date: '2026-03-04' } },
          { label: 'with +07:00 offset', query: { start_date: '2026-03-02T14:00:00+07:00', end_date: '2026-03-03T14:00:00+07:00' } },
          { label: 'only start', query: { start_date: '2026-03-05T00:00:00Z' } },
          { label: 'only end', query: { end_date: '2026-03-02T00:00:00Z' } },
        ],
      });

      const q = (label, query, token = 'admin_g1', expect) => get(label, '', token, { query, ...(expect ? { expect } : {}) });
      await q('created_by filter', { created_by: 'guard1@alpha.test' });
      await q('created_by is exact (partial does not match)', { created_by: 'guard1' });
      await q('created_by upper-case email is lowercased', { created_by: 'GUARD1@ALPHA.TEST' });
      await q('created_by empty -> 422', { created_by: '' }, 'admin_g1', 422);
      await q('created_by + search', { created_by: 'guard2@alpha.test', search: 'zaki' });
      await q('is_active=true', { is_active: 'true' });
      await q('is_active=false', { is_active: 'false' });
      await q('is_active=1', { is_active: '1' });
      await q('is_active=0', { is_active: '0' });
      await q('is_active=TRUE', { is_active: 'TRUE' });
      await q('is_active=yes', { is_active: 'yes' });
      await q('is_active=abc', { is_active: 'abc' });
      await q('is_active empty', { is_active: '' });
      await q('is_active true + paging', { is_active: 'true', limit: 2, offset: 2 });
      await q('is_active false + created_by + window', { is_active: 'false', created_by: 'guard1@alpha.test', start_date: '2026-03-01', end_date: '2026-03-10' });
      await q('guard sees the whole group', { limit: 100 }, 'guard1_g1');
      await q('resident sees the whole group', { limit: 3 }, 'resident_g1');

      // detail
      await get('by numeric id', '/1', 'admin_g1', { expect: 200 });
      await get('by qr uid', '/QR-A-0001', 'admin_g1', { expect: 200 });
      await get('qr uid is case-insensitive', '/qr-a-0003', 'admin_g1');
      await get('id that is also a qr uid', '/12', 'admin_g1', { expect: 200 });
      await get('guest with zero ack date', '/6', 'admin_g1', { expect: 200 });
      await get('guest with junk "N/A"/"null"/blank values', '/4', 'admin_g1', { expect: 200 });
      await get('checked-out guest', '/3', 'admin_g1', { expect: 200 });
      await get('guest of another group -> 404', '/13', 'admin_g1', { expect: 404 });
      await get('same guest from its own group', '/13', 'admin_g2', { expect: 200 });
      await get('unknown numeric id', '/99999', 'admin_g1', { expect: 404 });
      await get('id 0', '/0', 'admin_g1', { expect: 404 });
      await get('negative id matches nothing', '/-1', 'admin_g1', { expect: 404 });
      await get('unknown qr uid', '/QR-TIDAK-ADA', 'admin_g1', { expect: 404 });
      await get('id with a dot violates the pattern', '/1.5', 'admin_g1', { expect: 422 });
      await get('id with a space violates the pattern', '/a%20b', 'admin_g1', { expect: 422 });
      await get('id longer than 64 chars', `/${'a'.repeat(65)}`, 'admin_g1', { expect: 422 });
      await get('id of exactly 64 chars', `/${'a'.repeat(64)}`, 'admin_g1', { expect: 404 });
      await get('id with unicode', '/%C3%A1', 'admin_g1', { expect: 422 });
      await get('user without a group gets data:null (200)', '/1', 'user_nogroup', { expect: 200 });
      await get('detail without token', '/1', null, { expect: 401 });
      await get('detail with invalid id and no token (validation first)', '/a%20b', null);
      await get('resident can read a detail', '/2', 'resident_g1', { expect: 200 });
    },
  },
  {
    name: 'guest/create',
    order: 31,
    async run(t) {
      const create = (label, body, token = 'guard1_g1', extra = {}) =>
        t.step(label, { method: 'POST', path: V1, token, body, ...extra });

      await create('minimal body', base, 'guard1_g1', { expect: 201, capture: { g1: 'data.id', q1: 'data.qr_uid' } });
      await create('full body', {
        ...base, guest_name: 'Tamu Lengkap', guest_purpose: 'Rapat', guest_number: 'B 9999 ZZ', guest_identity: '3201020202020002',
        guest_identity_type: 'KTP', guest_address: 'Jl. Kenanga 7', guest_to_user_email: 'resident@alpha.test',
        guest_photo: 'https://cdn.example.test/g/new.jpg', guest_dt: '2026-03-10T08:00:00+07:00',
      }, 'guard1_g1', { expect: 201, capture: { g2: 'data.id' } });
      await create('type by uid resolves to the canonical name', { ...base, guest_type: 'gt-a-kurir' }, 'guard2_g1', { expect: 201 });
      await create('type match is case-insensitive and canonicalised', { ...base, guest_type: 'tamu umum' }, 'admin_g1', { expect: 201 });
      await create('host email in upper case is lowercased', { ...base, guest_to_user_email: 'RESIDENT2@ALPHA.TEST' }, 'guard1_g1', { expect: 201 });
      await create('nullable fields as null', { ...base, guest_purpose: null, guest_number: null, guest_identity: null, guest_identity_type: null, guest_address: null, guest_to_user_email: null, guest_photo: null, guest_dt: null }, 'guard1_g1', { expect: 201 });
      await create('supervisor (guard with allow_admin) may create', base, 'supervisor_g1', { expect: 201 });
      await create('creator email/name come from the token (case + trim normalised)', base, 'guard_upper_email', { expect: 201 });
      await create('guest_dt invalid text falls back to now', { ...base, guest_dt: 'bukan tanggal' }, 'guard1_g1', { expect: 201 });
      await create('guest_dt epoch-zero falls back to now', { ...base, guest_dt: '1970-01-01T00:00:00Z' }, 'guard1_g1', { expect: 201 });
      await create('guest_dt in the far future is kept', { ...base, guest_dt: '2099-01-01T00:00:00Z' }, 'guard1_g1', { expect: 201 });
      await create('guest_dt date only', { ...base, guest_dt: '2026-04-01' }, 'guard1_g1', { expect: 201 });
      await create('junk values "N/A" in optional fields become null', { ...base, guest_purpose: 'N/A', guest_number: 'null', guest_address: '  ' }, 'guard1_g1', { expect: 201 });
      await create('values are trimmed', { ...base, guest_name: '  Tamu Spasi  ', guest_destination: '  Blok Spasi ' }, 'guard1_g1', { expect: 201 });
      await create('extra fields are dropped', { ...base, qr_uid: 'diri-sendiri', group_name: 'x', id: '77' }, 'guard1_g1', { expect: 201 });
      await create('unicode and emoji', { ...base, guest_name: 'Zoë 山田 😀' }, 'guard1_g1', { expect: 201 });
      await create('max length name (128)', { ...base, guest_name: 'n'.repeat(128) }, 'guard1_g1', { expect: 201 });
      await create('max length address (512)', { ...base, guest_address: 'a'.repeat(512) }, 'guard1_g1', { expect: 201 });

      // validation (422) -- TypeBox
      const bad = (label, patch, token = 'guard1_g1') => create(label, { ...base, ...patch }, token, { expect: 422 });
      await bad('name too long', { guest_name: 'n'.repeat(129) });
      await bad('destination too long', { guest_destination: 'd'.repeat(129) });
      await bad('purpose too long', { guest_purpose: 'p'.repeat(129) });
      await bad('number too long', { guest_number: 'n'.repeat(33) });
      await bad('identity too long', { guest_identity: 'i'.repeat(65) });
      await bad('address too long', { guest_address: 'a'.repeat(513) });
      await bad('photo too long', { guest_photo: 'p'.repeat(2049) });
      await bad('type too long', { guest_type: 't'.repeat(65) });
      await bad('name empty', { guest_name: '' });
      await bad('destination empty', { guest_destination: '' });
      await bad('type empty', { guest_type: '' });
      await bad('name wrong type', { guest_name: 5 });
      await bad('identity type lowercase enum', { guest_identity_type: 'ktp' });
      await bad('identity type unknown enum', { guest_identity_type: 'NPWP' });
      await bad('host email invalid', { guest_to_user_email: 'bukan-email' });
      await bad('host email too long', { guest_to_user_email: `${'a'.repeat(120)}@alpha.test` });
      await bad('guest_dt wrong type', { guest_dt: 12345 });
      await create('name missing', { guest_destination: 'x', guest_type: 'Tamu Umum' }, 'guard1_g1', { expect: 422 });
      await create('destination missing', { guest_name: 'x', guest_type: 'Tamu Umum' }, 'guard1_g1', { expect: 422 });
      await create('type missing', { guest_name: 'x', guest_destination: 'y' }, 'guard1_g1', { expect: 422 });
      await create('empty object', {}, 'guard1_g1', { expect: 422 });
      await create('array body', [], 'guard1_g1', { expect: 422 });
      await create('null body', null, 'guard1_g1', { expect: 422 });
      await t.step('malformed json', { method: 'POST', path: V1, token: 'guard1_g1', rawBody: '{"guest_name":', expect: 400 });
      await t.step('no body', { method: 'POST', path: V1, token: 'guard1_g1' });
      await t.step('text/plain body', { method: 'POST', path: V1, token: 'guard1_g1', rawBody: JSON.stringify(base), headers: { 'content-type': 'text/plain' } });

      // business rules (400)
      await create('name only whitespace', { ...base, guest_name: '   ' }, 'guard1_g1', { expect: 400 });
      await create('name "N/A"', { ...base, guest_name: 'N/A' }, 'guard1_g1', { expect: 400 });
      await create('destination only whitespace', { ...base, guest_destination: '   ' }, 'guard1_g1', { expect: 400 });
      await create('type only whitespace', { ...base, guest_type: '   ' }, 'guard1_g1', { expect: 400 });
      await create('type not registered in the group', { ...base, guest_type: 'Tamu VIP' }, 'guard1_g1', { expect: 400 });
      await create('type of another group by uid', { ...base, guest_type: 'gt-b-vip' }, 'guard1_g1', { expect: 400 });
      await create('host is not a member', { ...base, guest_to_user_email: 'admin@beta.test' }, 'guard1_g1', { expect: 400 });
      await create('host membership is pending', { ...base, guest_to_user_email: 'pending@alpha.test' }, 'guard1_g1', { expect: 400 });
      await create('host unknown', { ...base, guest_to_user_email: 'hantu@alpha.test' }, 'guard1_g1', { expect: 400 });
      await create('no active group', base, 'admin_nogroup', { expect: 400 });

      // auth
      await create('resident may not create', base, 'resident_g1', { expect: 403 });
      await create('role VIEWER may not create', base, 'viewer_role', { expect: 403 });
      await create('no token', base, null, { expect: 401 });
      await create('no token + invalid body: validation runs first', {}, null);
      await create('resident + invalid body: validation runs first', {}, 'resident_g1');
      await create('stale session', base, 'stale_user', { expect: 401 });

      // read back what was created
      await t.step('detail of the minimal guest', { path: (v) => `${V1}/${v.g1}`, token: 'admin_g1', expect: 200 });
      await t.step('detail by generated qr uid', { path: (v) => `${V1}/${v.q1}`, token: 'admin_g1', expect: 200 });
      await t.step('detail of the full guest', { path: (v) => `${V1}/${v.g2}`, token: 'resident_g1', expect: 200 });
      await t.step('list shows new guests first (guest_dt desc)', { path: V1, token: 'admin_g1', query: { limit: 8 } });
      await t.step('creator filter sees only own guests', { path: V1, token: 'admin_g1', query: { created_by: 'guard1@alpha.test', limit: 50 } });
      await t.step('creator filter, normalised creator email', { path: V1, token: 'admin_g1', query: { created_by: 'guard1@alpha.test', search: 'spasi' } });
      await t.step('other tenant does not see the new guests', { path: V1, token: 'admin_g2', query: { limit: 50 } });
    },
  },
  {
    name: 'guest/acknowledge-checkout',
    order: 32,
    async run(t) {
      const mk = (label, body, key, token = 'guard1_g1') =>
        t.step(label, { method: 'POST', path: V1, token, body, expect: 201, capture: { [key]: 'data.id', [`${key}q`]: 'data.qr_uid' } });
      await mk('guest a (host: resident)', { ...base, guest_to_user_email: 'resident@alpha.test' }, 'a');
      await mk('guest b (host: resident2)', { ...base, guest_to_user_email: 'resident2@alpha.test' }, 'b');
      await mk('guest c (no host)', base, 'c');
      await mk('guest d (no host)', base, 'd');

      const ack = (label, id, body, token, extra = {}) =>
        t.step(label, { method: 'PATCH', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}/acknowledge`, token, body, ...extra });
      await ack('non-host resident cannot acknowledge', (v) => v.a, {}, 'resident2_g1', { expect: 403 });
      await ack('host acknowledges own visitor', (v) => v.a, {}, 'resident_g1', { expect: 200 });
      await ack('acknowledging twice -> 400', (v) => v.a, {}, 'resident_g1', { expect: 400 });
      await ack('staff acknowledges (guard)', (v) => v.b, {}, 'guard2_g1', { expect: 200 });
      await ack('admin acknowledges via qr in the path', (v) => v.cq, {}, 'admin_g1', { expect: 200 });
      await ack('qr_uid in the body wins over the path id', (v) => 'tidak-ada', (v) => ({ qr_uid: v.dq }), 'admin_g1', { expect: 200 });
      await ack('qr_uid null in body falls back to path', 'QR-A-0001', { qr_uid: null }, 'admin_g1', { expect: 200 });
      await ack('already acknowledged by qr', 'QR-A-0001', {}, 'admin_g1', { expect: 400 });
      await ack('acknowledging a checked-out guest -> 400', 3, {}, 'admin_g1', { expect: 400 });
      await ack('guest with zero ack date can be acknowledged', 6, {}, 'admin_g1', { expect: 200 });
      await ack('unknown id -> 404', 99999, {}, 'admin_g1', { expect: 404 });
      await ack('guest of another group -> 404', 13, {}, 'admin_g1', { expect: 404 });
      await ack('qr_uid too long -> 422', 2, { qr_uid: 'q'.repeat(65) }, 'admin_g1', { expect: 422 });
      await ack('qr_uid wrong type -> 422', 2, { qr_uid: 5 }, 'admin_g1', { expect: 422 });
      await ack('invalid id pattern -> 422', 'a%20b', {}, 'admin_g1', { expect: 422 });
      await ack('no token -> 401', 2, {}, null, { expect: 401 });
      await ack('no active group -> 400', 2, {}, 'admin_nogroup', { expect: 400 });
      await t.step('acknowledge with no body at all', { method: 'PATCH', path: `${V1}/10/acknowledge`, token: 'admin_g1' });
      await t.step('acknowledge with malformed json', { method: 'PATCH', path: `${V1}/10/acknowledge`, token: 'admin_g1', rawBody: '{', expect: 400 });

      const out = (label, id, body, token, extra = {}) =>
        t.step(label, { method: 'PATCH', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}/checkout`, token, body, ...extra });
      await out('resident cannot check out', (v) => v.a, {}, 'resident_g1', { expect: 403 });
      await out('guard checks out', (v) => v.a, {}, 'guard1_g1', { expect: 200 });
      await out('checking out twice -> 400', (v) => v.a, {}, 'guard1_g1', { expect: 400 });
      await out('admin checks out via qr path', (v) => v.bq, {}, 'admin_g1', { expect: 200 });
      await out('qr_uid in the body wins over the path id', 'tidak-ada', (v) => ({ qr_uid: v.cq }), 'guard2_g1', { expect: 200 });
      await out('checkout a never-acknowledged guest', (v) => v.d, {}, 'guard2_g1', { expect: 200 });
      await out('unknown id -> 404', 99999, {}, 'guard1_g1', { expect: 404 });
      await out('other group -> 404', 13, {}, 'guard1_g1', { expect: 404 });
      await out('qr_uid too long -> 422', 2, { qr_uid: 'q'.repeat(65) }, 'guard1_g1', { expect: 422 });
      await out('no token -> 401', 2, {}, null, { expect: 401 });
      await out('invalid id + no token -> validation first', 'a%20b', {}, null);
      await out('no active group -> 400', 2, {}, 'admin_nogroup', { expect: 400 });
      await t.step('state after the dance', { path: V1, token: 'admin_g1', query: { limit: 6 } });
      await t.step('active filter after the dance', { path: V1, token: 'admin_g1', query: { is_active: 'true', limit: 100 } });
      await t.step('inactive filter after the dance', { path: V1, token: 'admin_g1', query: { is_active: 'false', limit: 100 } });
    },
  },
  {
    name: 'guest/update',
    order: 33,
    async run(t) {
      const mk = (label, body, key, token) =>
        t.step(label, { method: 'POST', path: V1, token, body, expect: 201, capture: { [key]: 'data.id' } });
      await mk('guest owned by guard1', { ...base, guest_name: 'Milik Budi', guest_purpose: 'Awal', guest_address: 'Alamat Awal', guest_to_user_email: 'resident@alpha.test' }, 'own', 'guard1_g1');
      await mk('guest owned by guard2', { ...base, guest_name: 'Milik Citra' }, 'other', 'guard2_g1');

      for (const method of ['PUT', 'PATCH']) {
        const up = (label, id, body, token = 'guard1_g1', extra = {}) =>
          t.step(`${method} ${label}`, { method, path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}`, token, body, ...extra });
        await up('rename + purpose', (v) => v.own, { guest_name: 'Budi Diubah', guest_purpose: 'Rapat RT' }, 'guard1_g1', { expect: 200 });
        await up('type by name (case-insensitive) is canonicalised', (v) => v.own, { guest_type: 'kurir' }, 'guard1_g1', { expect: 200 });
        await up('type by uid', (v) => v.own, { guest_type: 'gt-a-ojek' }, 'guard1_g1', { expect: 200 });
        await up('clear nullable fields with null', (v) => v.own, { guest_purpose: null, guest_address: null, guest_to_user_email: null, guest_photo: null, guest_identity_type: null }, 'guard1_g1', { expect: 200 });
        await up('set identity fields', (v) => v.own, { guest_identity: '123', guest_identity_type: 'SIM', guest_number: 'B 1 A' }, 'guard1_g1', { expect: 200 });
        await up('whitespace name is ignored (no change)', (v) => v.own, { guest_name: '   ' }, 'guard1_g1', { expect: 200 });
        await up('"N/A" destination is ignored (no change)', (v) => v.own, { guest_destination: 'N/A' }, 'guard1_g1', { expect: 200 });
        await up('whitespace purpose clears it', (v) => v.own, { guest_purpose: '   ' }, 'guard1_g1', { expect: 200 });
        await up('empty body is a no-op', (v) => v.own, {}, 'guard1_g1', { expect: 200 });
        await up('guest_type null -> 422 (not nullable on update)', (v) => v.own, { guest_type: null }, 'guard1_g1', { expect: 422 });
        await up('extra fields (guest_dt, qr_uid, id) are dropped', (v) => v.own, { guest_dt: '2030-01-01T00:00:00Z', qr_uid: 'x', id: '1' }, 'guard1_g1', { expect: 200 });
        await up('set a valid host', (v) => v.own, { guest_to_user_email: 'RESIDENT2@alpha.test' }, 'guard1_g1', { expect: 200 });
        await up('update by qr uid in the path', 'QR-A-0001', { guest_purpose: 'via qr' }, 'admin_g1', { expect: 200 });
        await up('admin edits anyone', (v) => v.other, { guest_purpose: 'oleh admin' }, 'admin_g1', { expect: 200 });
        await up('supervisor (guard) does not get ownership by allow_admin', (v) => v.other, { guest_purpose: 'oleh supervisor' }, 'supervisor_g1', { expect: 403 });
        await up('guard may not edit another guard\'s guest', (v) => v.other, { guest_name: 'x' }, 'guard1_g1', { expect: 403 });
        await up('resident may not edit', (v) => v.own, { guest_name: 'x' }, 'resident_g1', { expect: 403 });
        await up('unknown type -> 400', (v) => v.own, { guest_type: 'Tidak Ada' }, 'guard1_g1', { expect: 400 });
        await up('empty type -> 422', (v) => v.own, { guest_type: '' }, 'guard1_g1', { expect: 422 });
        await up('whitespace type is cleaned to null and ignored', (v) => v.own, { guest_type: '   ' }, 'guard1_g1', { expect: 200 });
        await up('host is not a member -> 400', (v) => v.own, { guest_to_user_email: 'admin@beta.test' }, 'guard1_g1', { expect: 400 });
        await up('empty name -> 422', (v) => v.own, { guest_name: '' }, 'guard1_g1', { expect: 422 });
        await up('name too long -> 422', (v) => v.own, { guest_name: 'n'.repeat(129) }, 'guard1_g1', { expect: 422 });
        await up('identity type bad enum -> 422', (v) => v.own, { guest_identity_type: 'xx' }, 'guard1_g1', { expect: 422 });
        await up('host email invalid -> 422', (v) => v.own, { guest_to_user_email: 'x' }, 'guard1_g1', { expect: 422 });
        await up('unknown id -> 404', 99999, { guest_name: 'x' }, 'admin_g1', { expect: 404 });
        await up('other group -> 404', 13, { guest_name: 'x' }, 'admin_g1', { expect: 404 });
        await up('invalid id -> 422', 'a%20b', { guest_name: 'x' }, 'admin_g1', { expect: 422 });
        await up('no token -> 401', 1, { guest_name: 'x' }, null, { expect: 401 });
        await up('no active group -> 400', 1, { guest_name: 'x' }, 'admin_nogroup', { expect: 400 });
        await up('malformed json', 1, undefined, 'admin_g1', { rawBody: '{', expect: 400 });
      }
      await t.step('final state of the edited guest', { path: (v) => `${V1}/${v.own}`, token: 'admin_g1', expect: 200 });
      await t.step('final state of the other guest', { path: (v) => `${V1}/${v.other}`, token: 'admin_g1', expect: 200 });
    },
  },
  {
    name: 'guest/delete',
    order: 34,
    async run(t) {
      const mk = (label, key, token, name = 'Hapus') =>
        t.step(label, { method: 'POST', path: V1, token, body: { ...base, guest_name: name }, expect: 201, capture: { [key]: 'data.id', [`${key}q`]: 'data.qr_uid' } });
      for (const [k, tok] of [['a', 'guard1_g1'], ['b', 'guard1_g1'], ['c', 'guard2_g1'], ['d', 'guard1_g1'], ['e', 'guard1_g1'], ['f', 'admin_g1'], ['g', 'guard1_g1'], ['h', 'guard1_g1']]) {
        await mk(`create ${k}`, k, tok, `Hapus ${k}`);
      }

      const del = (label, id, token, extra = {}) =>
        t.step(label, { method: 'DELETE', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}`, token, ...extra });
      await del('guard deletes own guest', (v) => v.a, 'guard1_g1', { expect: 200 });
      await del('it is gone', (v) => v.a, 'guard1_g1', { expect: 404 });
      await t.step('detail after delete -> 404', { path: (v) => `${V1}/${v.a}`, token: 'admin_g1', expect: 404 });
      await del('guard cannot delete another guard\'s guest', (v) => v.c, 'guard1_g1', { expect: 403 });
      await del('resident cannot delete', (v) => v.b, 'resident_g1', { expect: 403 });
      await del('admin deletes anyone\'s', (v) => v.c, 'admin_g1', { expect: 200 });
      await del('supervisor (guard) can only delete own', (v) => v.f, 'supervisor_g1', { expect: 403 });
      await del('delete by qr uid is not supported (404)', (v) => v.bq, 'admin_g1', { expect: 404 });
      await del('unknown id -> 404', 99999, 'admin_g1', { expect: 404 });
      await del('other group -> 404', 13, 'admin_g1', { expect: 404 });
      await del('id 0 -> 404', 0, 'admin_g1', { expect: 404 });
      await del('invalid id -> 422', 'a%20b', 'admin_g1', { expect: 422 });
      await del('no token -> 401', 1, null, { expect: 401 });
      await del('no active group -> 400', 1, 'admin_nogroup', { expect: 400 });
      await t.step('seed guest of beta still exists', { path: `${V1}/13`, token: 'admin_g2', expect: 200 });

      // batch
      const batch = (label, ids, token, extra = {}) =>
        t.step(label, { method: 'DELETE', path: V1, token, body: typeof ids === 'function' ? ids : { ids }, ...extra });
      await batch('empty ids -> 422', [], 'admin_g1', { expect: 422 });
      await batch('non numeric id -> 422', ['abc'], 'admin_g1', { expect: 422 });
      await batch('id with spaces -> 422', [' 5 '], 'admin_g1', { expect: 422 });
      await batch('ids not an array -> 422', '5', 'admin_g1', { expect: 422 });
      await batch('ids number items -> 422', [5], 'admin_g1', { expect: 422 });
      await t.step('missing ids -> 422', { method: 'DELETE', path: V1, token: 'admin_g1', body: {}, expect: 422 });
      await t.step('no body -> 422', { method: 'DELETE', path: V1, token: 'admin_g1' });
      await t.step('malformed json -> 400', { method: 'DELETE', path: V1, token: 'admin_g1', rawBody: '{', expect: 400 });
      await batch('id 0 matches nothing -> 404', ['0'], 'admin_g1', { expect: 404 });
      await batch('mix of existing and unknown -> 404 and nothing is deleted', (v) => ({ ids: [v.d, '99999'] }), 'admin_g1', { expect: 404 });
      await t.step('guest d still exists', { path: (v) => `${V1}/${v.d}`, token: 'admin_g1', expect: 200 });
      await batch('guard with someone else\'s id -> 403', (v) => ({ ids: [v.d, v.f] }), 'guard1_g1', { expect: 403 });
      await batch('ids from another group -> 404', ['13', '14'], 'admin_g1', { expect: 404 });
      await batch('resident -> 403', (v) => ({ ids: [v.d] }), 'resident_g1', { expect: 403 });
      await batch('no token -> 401', ['1'], null, { expect: 401 });
      await batch('no active group -> 400', ['1'], 'admin_nogroup', { expect: 400 });
      await batch('duplicates are collapsed', (v) => ({ ids: [v.d, v.d, v.e] }), 'guard1_g1', { expect: 200 });
      await batch('already deleted ids -> 404', (v) => ({ ids: [v.d] }), 'guard1_g1', { expect: 404 });
      await batch('admin batch of three', (v) => ({ ids: [v.f, v.g, v.h] }), 'admin_g1', { expect: 200 });
      await batch('leading zeros are numeric ids', ['0000012'], 'admin_g1', { expect: 200 });
      await t.step('list after the deletions', { path: V1, token: 'admin_g1', query: { limit: 100 } });
    },
  },
];

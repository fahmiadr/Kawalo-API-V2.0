// Information feed: list/search, create, update (PUT+PATCH), delete (single + batch).
import { listMatrix } from '../lib/cases.mjs';

const V1 = '/api/v1/information';
const base = { title: 'Pengumuman Uji' };

export default [
  {
    name: 'information/read',
    order: 40,
    async run(t) {
      await listMatrix(t, {
        path: V1,
        token: 'admin_g1',
        tenants: [
          { label: 'beta (has data)', token: 'admin_g2' },
          { label: 'gamma (no data)', token: 'admin_g3' },
          { label: 'no active group', token: 'user_nogroup' },
        ],
        searches: ['kerja bakti', 'listrik', 'helm', 'blok c', 'event', 'general', 'andi', 'guard1@alpha', 'lapangan', '100%', 'wildcard'],
        dates: [
          { label: 'single day (utc)', query: { start_date: '2026-03-01T00:00:00Z', end_date: '2026-03-01T23:59:59Z' } },
          { label: 'date only', query: { start_date: '2026-03-02', end_date: '2026-03-04' } },
          { label: 'with +07:00 offset', query: { start_date: '2026-03-03T05:00:00+07:00', end_date: '2026-03-06T00:00:00+07:00' } },
          { label: 'only start', query: { start_date: '2026-03-05T00:00:00Z' } },
          { label: 'only end', query: { end_date: '2026-03-02T00:00:00Z' } },
        ],
      });
      const q = (label, query, token = 'admin_g1', expect) => t.step(label, { path: V1, token, query, ...(expect ? { expect } : {}) });
      await q('created_by exact', { created_by: 'admin@alpha.test' });
      await q('created_by partial does not match', { created_by: 'admin' });
      await q('created_by uppercase is lowercased', { created_by: 'ADMIN@ALPHA.TEST' });
      await q('created_by empty -> 422', { created_by: '' }, 'admin_g1', 422);
      await q('created_by + search + window', { created_by: 'guard1@alpha.test', search: 'jadwal', start_date: '2026-03-01', end_date: '2026-03-31' });
      await q('guard reads the whole group', { limit: 100 }, 'guard1_g1');
      await q('resident reads the whole group', { limit: 4 }, 'resident_g1');
    },
  },
  {
    name: 'information/create',
    order: 41,
    async run(t) {
      const create = (label, body, token = 'admin_g1', extra = {}) => t.step(label, { method: 'POST', path: V1, token, body, ...extra });
      await create('minimal', base, 'admin_g1', { expect: 201, capture: { i1: 'data.id' } });
      await create('guard creates, name/profile pic from the account', { ...base, text: 'Dari penjaga' }, 'guard1_g1', { expect: 201 });
      await create('guard without profile pic in the account', { ...base }, 'guard2_g1', { expect: 201 });
      await create('full body', {
        title: 'Lengkap', text: 'Isi teks', feed_file: 'https://cdn.example.test/f/doc.pdf', photos: ['https://a/1.jpg', 'https://a/2.jpg', 'https://a/3.jpg', 'https://a/4.jpg'],
        feed_type: 'EVENT', event_location: 'Balai Warga', glat: '-6.2088', glong: '106.8456',
      }, 'admin_g1', { expect: 201, capture: { i2: 'data.id' } });
      for (const ft of ['GENERAL', 'JOIN', 'INCIDENT', 'EVENT', 'OTHER']) await create(`feed_type ${ft}`, { ...base, feed_type: ft }, 'admin_g1', { expect: 201 });
      await create('feed_type null defaults to OTHER', { ...base, feed_type: null }, 'admin_g1', { expect: 201 });
      await create('nullable fields as null', { ...base, text: null, feed_file: null, event_location: null, glat: null, glong: null }, 'admin_g1', { expect: 201 });
      await create('photos empty array', { ...base, photos: [] }, 'admin_g1', { expect: 201 });
      await create('photos with blanks and N/A slide to the left', { ...base, photos: ['', 'https://a/x.jpg', 'N/A', 'https://a/y.jpg'] }, 'admin_g1', { expect: 201 });
      await create('photos: 2 valid among 4 junk', { ...base, photos: ['null', 'https://a/z.jpg', '', ' '] }, 'admin_g1', { expect: 201 });
      await create('coordinate "0" and "0.0" are dropped', { ...base, glat: '0', glong: '0.0' }, 'admin_g1', { expect: 201 });
      await create('coordinate not a number is dropped', { ...base, glat: 'abc', glong: '12abc' }, 'admin_g1', { expect: 201 });
      await create('coordinate with spaces is trimmed', { ...base, glat: ' -6.2 ', glong: ' 106.8 ' }, 'admin_g1', { expect: 201 });
      await create('coordinate scientific notation', { ...base, glat: '1e1', glong: '-1.5E+2' }, 'admin_g1', { expect: 201 });
      await create('values are trimmed and N/A text dropped', { title: '  Judul Spasi  ', text: 'N/A', event_location: '  Taman  ' }, 'admin_g1', { expect: 201 });
      await create('unknown fields are dropped', { ...base, feed_dt: '2000-01-01T00:00:00Z', user_email: 'x@y.z', id: '5' }, 'admin_g1', { expect: 201 });
      await create('title at max length 128', { title: 't'.repeat(128) }, 'admin_g1', { expect: 201 });
      await create('text at max length 1024', { ...base, text: 'x'.repeat(1024) }, 'admin_g1', { expect: 201 });
      await create('unicode', { title: 'Hujan 🌧 deras – 山田', text: 'Waspada banjir' }, 'admin_g1', { expect: 201 });

      const bad = (label, body, token = 'admin_g1') => create(label, body, token, { expect: 422 });
      await bad('title missing', {});
      await bad('title empty', { title: '' });
      await bad('title too long', { title: 't'.repeat(129) });
      await bad('title wrong type', { title: 5 });
      await bad('title null', { title: null });
      await bad('text too long', { ...base, text: 'x'.repeat(1025) });
      await bad('feed_file too long', { ...base, feed_file: 'x'.repeat(2049) });
      await bad('photos more than 4', { ...base, photos: ['a', 'b', 'c', 'd', 'e'] });
      await bad('photos item too long', { ...base, photos: ['x'.repeat(2049)] });
      await bad('photos not an array', { ...base, photos: 'a.jpg' });
      await bad('photos null', { ...base, photos: null });
      await bad('photos item wrong type', { ...base, photos: [1] });
      await bad('feed_type lowercase', { ...base, feed_type: 'general' });
      await bad('feed_type unknown', { ...base, feed_type: 'NEWS' });
      await bad('event_location too long', { ...base, event_location: 'x'.repeat(129) });
      await bad('glat too long', { ...base, glat: '1'.repeat(33) });
      await bad('glat as number', { ...base, glat: -6.2 });
      await bad('array body', []);
      await bad('null body', null);
      await create('title whitespace only -> 400 from the use case', { title: '   ' }, 'admin_g1', { expect: 400 });
      await create('title "N/A" -> 400', { title: 'N/A' }, 'admin_g1', { expect: 400 });
      await create('no active group -> 400', base, 'admin_nogroup', { expect: 400 });
      await create('resident may not create', base, 'resident_g1', { expect: 403 });
      await create('no token', base, null, { expect: 401 });
      await create('no token + invalid body: validation first', {}, null);
      await create('resident + invalid body: validation first', {}, 'resident_g1');
      await t.step('malformed json', { method: 'POST', path: V1, token: 'admin_g1', rawBody: '{"title"', expect: 400 });
      await t.step('no body', { method: 'POST', path: V1, token: 'admin_g1' });

      await t.step('list shows the new items first', { path: V1, token: 'admin_g1', query: { limit: 6 } });
      await t.step('search finds the created unicode item', { path: V1, token: 'admin_g1', query: { search: 'hujan' } });
      await t.step('other tenant does not see them', { path: V1, token: 'admin_g2', query: { limit: 100 } });
    },
  },
  {
    name: 'information/update',
    order: 42,
    async run(t) {
      const mk = (label, body, key, token) => t.step(label, { method: 'POST', path: V1, token, body, expect: 201, capture: { [key]: 'data.id' } });
      await mk('item of guard1', { title: 'Milik Budi', text: 'Awal', feed_type: 'GENERAL', photos: ['https://a/1.jpg', 'https://a/2.jpg'], event_location: 'Pos', glat: '-6.1', glong: '106.1' }, 'own', 'guard1_g1');
      await mk('item of guard2', { title: 'Milik Citra' }, 'other', 'guard2_g1');

      for (const method of ['PUT', 'PATCH']) {
        const up = (label, id, body, token = 'guard1_g1', extra = {}) =>
          t.step(`${method} ${label}`, { method, path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}`, token, body, ...extra });
        await up('title + text', (v) => v.own, { title: 'Judul Baru', text: 'Teks Baru' }, 'guard1_g1', { expect: 200 });
        await up('feed_type valid', (v) => v.own, { feed_type: 'EVENT' }, 'guard1_g1', { expect: 200 });
        await up('feed_type null is ignored', (v) => v.own, { feed_type: null }, 'guard1_g1', { expect: 200 });
        await up('title null is ignored', (v) => v.own, { title: null }, 'guard1_g1', { expect: 200 });
        await up('title whitespace is ignored', (v) => v.own, { title: '   ' }, 'guard1_g1', { expect: 200 });
        await up('text null clears it', (v) => v.own, { text: null }, 'guard1_g1', { expect: 200 });
        await up('text whitespace clears it', (v) => v.own, { text: '   ' }, 'guard1_g1', { expect: 200 });
        await up('photos replaced (slides left)', (v) => v.own, { photos: ['', 'https://a/n1.jpg', 'N/A', 'https://a/n2.jpg'] }, 'guard1_g1', { expect: 200 });
        await up('photos empty array clears all', (v) => v.own, { photos: [] }, 'guard1_g1', { expect: 200 });
        await up('photos absent leaves them untouched', (v) => v.own, { text: 'dengan foto' }, 'guard1_g1', { expect: 200 });
        await up('photos set again', (v) => v.own, { photos: ['https://a/p1.jpg', 'https://a/p2.jpg', 'https://a/p3.jpg', 'https://a/p4.jpg'] }, 'guard1_g1', { expect: 200 });
        await up('coordinates "0" clear them', (v) => v.own, { glat: '0', glong: '0' }, 'guard1_g1', { expect: 200 });
        await up('coordinates set', (v) => v.own, { glat: '-6.5', glong: '106.5' }, 'guard1_g1', { expect: 200 });
        await up('coordinates null clear them', (v) => v.own, { glat: null, glong: null }, 'guard1_g1', { expect: 200 });
        await up('event_location + feed_file', (v) => v.own, { event_location: 'Lapangan', feed_file: 'https://a/doc.pdf' }, 'guard1_g1', { expect: 200 });
        await up('clear event_location and feed_file', (v) => v.own, { event_location: null, feed_file: null }, 'guard1_g1', { expect: 200 });
        await up('empty body is a no-op', (v) => v.own, {}, 'guard1_g1', { expect: 200 });
        await up('unknown fields dropped', (v) => v.own, { feed_dt: '2000-01-01', user_email: 'x@y.z' }, 'guard1_g1', { expect: 200 });
        await up('admin edits anyone\'s', (v) => v.other, { title: 'Diubah admin' }, 'admin_g1', { expect: 200 });
        await up('guard may not edit another guard\'s', (v) => v.other, { title: 'x' }, 'guard1_g1', { expect: 403 });
        await up('supervisor (guard) may not edit others either', (v) => v.other, { title: 'x' }, 'supervisor_g1', { expect: 403 });
        await up('resident may not edit', (v) => v.own, { title: 'x' }, 'resident_g1', { expect: 403 });
        await up('seed item from another group -> 404', 9, { title: 'x' }, 'admin_g1', { expect: 404 });
        await up('unknown id -> 404', 99999, { title: 'x' }, 'admin_g1', { expect: 404 });
        await up('id 0 -> 404', 0, { title: 'x' }, 'admin_g1', { expect: 404 });
        await up('non numeric id -> 422', 'abc', { title: 'x' }, 'admin_g1', { expect: 422 });
        await up('id with spaces -> 422', '%201', { title: 'x' }, 'admin_g1', { expect: 422 });
        await up('empty title -> 422', (v) => v.own, { title: '' }, 'guard1_g1', { expect: 422 });
        await up('feed_type lowercase -> 422', (v) => v.own, { feed_type: 'general' }, 'guard1_g1', { expect: 422 });
        await up('too many photos -> 422', (v) => v.own, { photos: ['a', 'b', 'c', 'd', 'e'] }, 'guard1_g1', { expect: 422 });
        await up('photos null -> 422', (v) => v.own, { photos: null }, 'guard1_g1', { expect: 422 });
        await up('no token -> 401', 1, { title: 'x' }, null, { expect: 401 });
        await up('no active group -> 400', 1, { title: 'x' }, 'admin_nogroup', { expect: 400 });
        await up('malformed json', 1, undefined, 'admin_g1', { rawBody: '{', expect: 400 });
      }
      await t.step('final state (own)', { path: V1, token: 'admin_g1', query: { search: 'Judul Baru' } });
      await t.step('final state (all, newest first)', { path: V1, token: 'admin_g1', query: { limit: 4 } });
    },
  },
  {
    name: 'information/delete',
    order: 43,
    async run(t) {
      const mk = (key, token, title) => t.step(`create ${key}`, { method: 'POST', path: V1, token, body: { title }, expect: 201, capture: { [key]: 'data.id' } });
      for (const [k, tok] of [['a', 'guard1_g1'], ['b', 'guard1_g1'], ['c', 'guard2_g1'], ['d', 'guard1_g1'], ['e', 'guard1_g1'], ['f', 'admin_g1']]) await mk(k, tok, `Hapus ${k}`);

      const del = (label, id, token, extra = {}) => t.step(label, { method: 'DELETE', path: (v) => `${V1}/${typeof id === 'function' ? id(v) : id}`, token, ...extra });
      await del('guard deletes own', (v) => v.a, 'guard1_g1', { expect: 200 });
      await del('again -> 404', (v) => v.a, 'guard1_g1', { expect: 404 });
      await del('guard cannot delete another guard\'s', (v) => v.c, 'guard1_g1', { expect: 403 });
      await del('resident cannot delete', (v) => v.b, 'resident_g1', { expect: 403 });
      await del('admin deletes anyone\'s', (v) => v.c, 'admin_g1', { expect: 200 });
      await del('other group -> 404', 9, 'admin_g1', { expect: 404 });
      await del('unknown -> 404', 99999, 'admin_g1', { expect: 404 });
      await del('non numeric -> 422', 'abc', 'admin_g1', { expect: 422 });
      await del('no token -> 401', 1, null, { expect: 401 });
      await del('no active group -> 400', 1, 'admin_nogroup', { expect: 400 });

      const batch = (label, ids, token, extra = {}) => t.step(label, { method: 'DELETE', path: V1, token, body: typeof ids === 'function' ? ids : { ids }, ...extra });
      await batch('empty -> 422', [], 'admin_g1', { expect: 422 });
      await batch('non numeric -> 422', ['x'], 'admin_g1', { expect: 422 });
      await batch('mixture with unknown -> 404, nothing deleted', (v) => ({ ids: [v.d, '99999'] }), 'admin_g1', { expect: 404 });
      await batch('guard + foreign item -> 403', (v) => ({ ids: [v.d, v.f] }), 'guard1_g1', { expect: 403 });
      await batch('other group -> 404', ['9', '10'], 'admin_g1', { expect: 404 });
      await batch('resident -> 403', (v) => ({ ids: [v.d] }), 'resident_g1', { expect: 403 });
      await batch('no token -> 401', ['1'], null, { expect: 401 });
      await batch('duplicates collapse', (v) => ({ ids: [v.d, v.d, v.e] }), 'guard1_g1', { expect: 200 });
      await batch('already gone -> 404', (v) => ({ ids: [v.d] }), 'guard1_g1', { expect: 404 });
      await batch('admin single via batch', (v) => ({ ids: [v.f] }), 'admin_g1', { expect: 200 });
      await t.step('beta data is intact', { path: V1, token: 'admin_g2', query: { limit: 100 } });
      await t.step('list after the deletions', { path: V1, token: 'admin_g1', query: { limit: 100 } });
    },
  },
];

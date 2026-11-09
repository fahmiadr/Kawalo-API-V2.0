// Read-only endpoints: group members, attendance, operational.
import { listMatrix } from '../lib/cases.mjs';

const V1 = '/api/v1';
const tenants = [
  { label: 'beta (has data)', token: 'admin_g2' },
  { label: 'gamma (no data)', token: 'admin_g3' },
  { label: 'no active group', token: 'user_nogroup' },
];

export default [
  {
    name: 'group/members',
    order: 20,
    async run(t) {
      const get = (label, token, extra = {}) => t.step(label, { path: `${V1}/groups/members`, token, ...extra });
      await get('alpha admin: JOINED members sorted with localeCompare', 'admin_g1', { expect: 200 });
      await get('alpha guard sees the same list', 'guard1_g1', { expect: 200 });
      await get('alpha resident (USER role is allowed)', 'resident_g1', { expect: 200 });
      await get('beta members', 'admin_g2', { expect: 200 });
      await get('gamma members (utility users)', 'admin_g3', { expect: 200 });
      await get('no active group -> empty list', 'user_nogroup', { expect: 200 });
      await get('query string is ignored', 'guard1_g1', { query: { limit: 1, search: 'x' } });
      await get('no token', null, { expect: 401 });
      await get('stale session', 'stale_user', { expect: 401 });
      await get('expired token', 'expired', { expect: 401 });
      await t.step('POST not allowed', { method: 'POST', path: `${V1}/groups/members`, token: 'admin_g1', body: {}, expect: 404 });
      await t.step('GET /groups -> 404', { path: `${V1}/groups`, token: 'admin_g1', expect: 404 });
    },
  },
  {
    name: 'attendance/list',
    order: 21,
    async run(t) {
      await listMatrix(t, {
        path: `${V1}/attendance`,
        token: 'admin_g1',
        tenants,
        searches: ['budi', 'citra', 'check', 'masuk', 'pos utama', 'pagi', 'malam', 'REGISTER', 'unknown@alpha', 'aneh'],
        dates: [
          { label: 'single day (utc)', query: { start_date: '2026-03-01T00:00:00Z', end_date: '2026-03-01T23:59:59Z' } },
          { label: 'date only', query: { start_date: '2026-03-02', end_date: '2026-03-03' } },
          { label: 'with +07:00 offset', query: { start_date: '2026-03-01T13:00:00+07:00', end_date: '2026-03-02T13:00:00+07:00' } },
          { label: 'only start', query: { start_date: '2026-03-03T00:00:00Z' } },
          { label: 'only end', query: { end_date: '2026-03-01T23:00:00Z' } },
          { label: 'window + search', query: { start_date: '2026-03-01', end_date: '2026-03-05', search: 'check' } },
        ],
      });
      await t.step('guard role sees the whole group, not only own rows', { path: `${V1}/attendance`, token: 'guard1_g1', query: { limit: 100 } });
      await t.step('resident role may read', { path: `${V1}/attendance`, token: 'resident_g1', query: { limit: 3 } });
    },
  },
  {
    name: 'operational/list',
    order: 22,
    async run(t) {
      await listMatrix(t, {
        path: `${V1}/operational`,
        token: 'admin_g1',
        tenants,
        searches: ['patrol', 'guard2', 'budi', 'on duty', 'off duty', 'emergency', 'supervisor', 'citra'],
        dates: [
          { label: 'single day (utc)', query: { start_date: '2026-03-01T00:00:00Z', end_date: '2026-03-01T23:59:59Z' } },
          { label: 'date only range', query: { start_date: '2026-03-02', end_date: '2026-03-03' } },
          { label: 'with +07:00 offset', query: { start_date: '2026-03-01T13:00:00+07:00', end_date: '2026-03-02T13:00:00+07:00' } },
          { label: 'only start', query: { start_date: '2026-03-03T00:00:00Z' } },
          { label: 'only end', query: { end_date: '2026-03-01T23:00:00Z' } },
          { label: 'window crossing midnight', query: { start_date: '2026-03-03T20:00:00Z', end_date: '2026-03-04T02:00:00Z' } },
          { label: 'window + search', query: { start_date: '2026-03-01', end_date: '2026-03-05', search: 'patrol' } },
          { label: 'window + paging', query: { start_date: '2026-03-01', end_date: '2026-03-05', limit: 2, offset: 1 } },
        ],
      });
      await t.step('guard role may read', { path: `${V1}/operational`, token: 'guard1_g1', query: { limit: 100 } });
    },
  },
];

// Query-parameter matrix shared by every list endpoint (guests, information, attendance, operational, incidents).

/**
 * @param t scenario context
 * @param o.path        endpoint path
 * @param o.token       a token of the group that owns the seed data
 * @param o.tenants     [{label, token}] other tenants: one with data, one empty, one without a group
 * @param o.searches    search terms worth trying for this endpoint
 * @param o.dates       [{label, query}] date windows
 * @param o.extra       extra query params merged into each request (e.g. a fixed filter)
 */
export async function listMatrix(t, o) {
  const get = (label, query = {}, token = o.token, expect) =>
    t.step(label, { path: o.path, token, query: { ...(o.extra ?? {}), ...query }, ...(expect ? { expect } : {}) });

  await get('default page');
  await get('limit=1', { limit: 1 });
  await get('limit=2&offset=1', { limit: 2, offset: 1 });
  await get('limit=3&offset=3', { limit: 3, offset: 3 });
  await get('offset beyond the data', { limit: 5, offset: 500 });
  await get('limit=0 means unlimited', { limit: 0 });
  await get('limit=0 with offset (offset ignored)', { limit: 0, offset: 2 });
  await get('limit=100 (max)', { limit: 100 });
  await get('limit=101 -> 422', { limit: 101 }, o.token, 422);
  await get('limit=-1 -> 422', { limit: -1 }, o.token, 422);
  await get('limit=abc -> 422', { limit: 'abc' }, o.token, 422);
  await get('limit empty string', { limit: '' });
  await get('limit=2.5 (decimal)', { limit: 2.5 });
  await get('limit=1e1 (exponent)', { limit: '1e1' });
  await get('limit=0x5 (hex)', { limit: '0x5' });
  await get('limit with surrounding spaces', { limit: ' 2 ' });
  await get('limit given twice', { limit: [1, 2] });
  await get('offset=-1 -> 422', { offset: -1 }, o.token, 422);
  await get('offset=abc -> 422', { offset: 'abc' }, o.token, 422);
  await get('offset=1.5 (decimal)', { limit: 2, offset: 1.5 });
  await get('offset only', { offset: 2 });
  await get('unknown query params are ignored', { foo: 'bar', limit: 2 });
  await get('search empty -> 422', { search: '' }, o.token, 422);
  await get('search whitespace only is dropped', { search: '   ' });
  await get('search "N/A" is dropped by cleanString', { search: 'N/A' });
  await get('search "null" is dropped by cleanString', { search: 'null' });
  await get('search matches nothing', { search: 'zzzz-tidak-ada' });
  for (const term of o.searches ?? []) await get(`search "${term}"`, { search: term });
  await get('search is case-insensitive (upper)', { search: (o.searches?.[0] ?? 'a').toUpperCase() });
  await get('search with LIKE wildcard %', { search: '%' });
  await get('search with LIKE wildcard _', { search: '_' });
  await get('search literal 100%', { search: '100%' });
  await get('search with accent', { search: 'álvaro' });
  await get('search trimmed', { search: `  ${o.searches?.[0] ?? 'a'}  ` });
  await get('search with quote and backslash', { search: `a'b\\c"d` });
  await get('search with unicode', { search: 'ñ' });
  await get('search + paging', { search: o.searches?.[0] ?? 'a', limit: 1, offset: 1 });
  for (const w of o.dates ?? []) await get(`date window: ${w.label}`, w.query);
  await get('invalid start_date is ignored', { start_date: 'bukan-tanggal' });
  await get('epoch-zero start_date is ignored', { start_date: '1970-01-01T00:00:00Z' });
  await get('zero-date start_date is ignored', { start_date: '0000-00-00' });
  await get('numeric-string start_date is ignored', { start_date: '1772000000000' });
  await get('end_date before start_date', { start_date: '2026-03-05', end_date: '2026-03-01' });
  await get('start_date far in the future', { start_date: '2099-01-01' });
  await get('start_date == end_date (instant)', { start_date: '2026-03-01T08:00:00Z', end_date: '2026-03-01T08:00:00Z' });

  for (const tenant of o.tenants ?? []) await get(`tenant isolation: ${tenant.label}`, {}, tenant.token);
  await t.step('no token -> 401', { path: o.path, query: o.extra, expect: 401 });
  await t.step('expired token -> 401', { path: o.path, token: 'expired', expect: 401 });
  await t.step('stale session -> 401', { path: o.path, token: 'stale_user', expect: 401 });
  await t.step('invalid query beats missing token (validation first)', { path: o.path, query: { limit: -1 } });
}

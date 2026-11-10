// Records what the TypeScript server answers (`details` of a 422 in development) for a systematic set of bodies per
// request DTO, so the Rust validators can be replayed against it without the oracle (tests/typebox_details.rs).
//   cd tools/parity && $KW_TS_ROOT/node_modules/.bin/tsx --tsconfig $KW_TS_ROOT/tsconfig.json typebox-corpus.mts ../../tests/fixtures/typebox_details.json
// Needs KW_TS_ROOT (the legacy/typescript checkout); NODE_ENV is forced to development.
import { writeFileSync } from 'node:fs';
import { Elysia } from 'elysia';

process.env.NODE_ENV = 'development';
const root = process.env.KW_TS_ROOT;
if (!root) throw new Error('set KW_TS_ROOT (see tools/README.md)');
const load = (p: string) => import(`${root}/src/${p}`);
const { errorHandlerMiddleware } = await load('core/middleware/error-handler.middleware.ts');
const auth = await load('features/auth/presentation/dtos/auth-request.dto.ts');
const guest = await load('features/guest/presentation/dtos/guest-request.dto.ts');
const incident = await load('features/incident/presentation/dtos/incident-request.dto.ts');
const info = await load('features/information/presentation/dtos/information-request.dto.ts');

const dtos: Record<string, any> = {
  LoginDto: auth.LoginDto,
  RefreshTokenDto: auth.RefreshTokenDto,
  SwitchGroupDto: auth.SwitchGroupDto,
  LogoutDto: auth.LogoutDto,
  CreateGuestDto: guest.CreateGuestDto,
  UpdateGuestDto: guest.UpdateGuestDto,
  AcknowledgeGuestDto: guest.AcknowledgeGuestDto,
  DeleteBatchGuestDto: guest.DeleteBatchGuestDto,
  CreateIncidentDto: incident.CreateIncidentDto,
  PostIncidentReplyDto: incident.PostIncidentReplyDto,
  CreateInformationDto: info.CreateInformationDto,
  UpdateInformationDto: info.UpdateInformationDto,
  DeleteBatchInformationDto: info.DeleteBatchInformationDto,
};

const apps = Object.fromEntries(
  Object.entries(dtos).map(([name, body]) => [
    name,
    new Elysia().use(errorHandlerMiddleware).post('/', () => 'ok', { body }),
  ]),
);

async function details(name: string, raw: string): Promise<string | null> {
  const res = await apps[name]!.handle(
    new Request('http://localhost/', { method: 'POST', headers: { 'content-type': 'application/json' }, body: raw }),
  );
  if (res.status === 422) return (await res.json()).details ?? null;
  if (res.status !== 200) throw new Error(`${name} ${raw}: status ${res.status}`);
  return null;
}

// Long strings are stored as "\0<count>*<text>" to keep the fixture small; the Rust test expands them the same way.
const rep = (text: string, count: number) => (count < 16 ? text.repeat(count) : `\0${count}*${text}`);
const expand = (v: any): any => {
  if (typeof v === 'string') {
    const m = /^\0(\d+)\*(.*)$/s.exec(v);
    return m ? m[2]!.repeat(Number(m[1])) : v;
  }
  if (Array.isArray(v)) return v.map(expand);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, expand(x)]));
  return v;
};

type Node = { anyOf?: any[]; type?: string; const?: unknown; items?: any; [k: string]: any };
const isNull = (n: Node) => n.type === 'null';
const unwrap = (n: Node): Node => (n.anyOf?.some(isNull) ? n.anyOf.find((x) => !isNull(x))! : n);

function sample(n: Node): unknown {
  n = unwrap(n);
  if (n.anyOf) return n.anyOf[0].const;
  if (n.type === 'string') return n.format === 'email' ? 'a@b.co' : 'x';
  if (n.type === 'number') return 1;
  if (n.type === 'array') return [sample(n.items)];
  return null;
}

const wrongTypes: unknown[] = [null, true, 5, 1.5, -2, 1e21, 'txt', [], ['a', 2, null, [3, 4]], {}, { a: 1 }];

function badValues(n: Node): unknown[] {
  const out: unknown[] = [...wrongTypes];
  const b = unwrap(n);
  if (b.type === 'string') {
    if (b.minLength) out.push('');
    if (b.maxLength) out.push(rep('x', b.maxLength + 1), rep('😀', Math.floor(b.maxLength / 2) + 1), rep('😀', b.maxLength / 2));
    if (b.format === 'email') out.push('nope', 'a@b', ' a@b.co', 'a@b.co\n', rep('x', 130) + '@b.co');
    if (b.pattern) out.push('abc', '12a', '123\n', '', ' 1');
  } else if (b.type === 'number') {
    out.push(b.minimum - 1, b.maximum + 1, b.minimum, b.maximum, 0.5);
  } else if (b.type === 'array') {
    if (b.minItems) out.push([]);
    if (b.maxItems) out.push(Array.from({ length: b.maxItems + 1 }, () => sample(b.items)));
    for (const bad of badValues(b.items)) out.push([sample(b.items), bad], [bad, bad]);
    // Every item bad, with and without exceeding maxItems; 70 bad items exercises Elysia's cap of 64 errors.
    const worst = b.items.pattern ? 'abc' : rep('x', b.items.maxLength + 1);
    const count = (b.maxItems ?? 3) + 1;
    out.push(Array.from({ length: count }, () => worst), Array.from({ length: count }, () => 5));
    if (!b.maxItems) out.push(Array.from({ length: 70 }, () => worst));
  } else if (b.anyOf) {
    out.push('NOPE', 'ktp', (b.anyOf[0].const as string).toLowerCase() + 'x');
  }
  return out;
}

const cases = new Map<string, Set<string>>();
const add = (dto: string, body: unknown) => {
  const raw = JSON.stringify(body);
  if (!cases.has(dto)) cases.set(dto, new Set());
  cases.get(dto)!.add(raw);
};

for (const [name, schema] of Object.entries(dtos)) {
  const props: Record<string, Node> = schema.properties;
  const keys = Object.keys(props);
  const required: string[] = schema.required ?? [];
  const valid: Record<string, unknown> = {};
  for (const k of required) valid[k] = sample(props[k]!);

  for (const wholly of [null, true, 5, 'txt', [], [1], {}, { zzz: 1 }, valid]) add(name, wholly);
  for (const k of keys) {
    add(name, { ...valid, [k]: sample(props[k]!) });
    const without = { ...valid };
    delete without[k];
    add(name, without);
    for (const bad of badValues(props[k]!)) add(name, { ...valid, [k]: bad });
  }
  // Several failing properties at once: schema order must win over body order.
  const allBad: Record<string, unknown> = {};
  const allBadReversed: Record<string, unknown> = {};
  for (const k of keys) allBad[k] = 5;
  for (const k of [...keys].reverse()) allBadReversed[k] = [];
  add(name, allBad);
  add(name, allBadReversed);
  add(name, Object.fromEntries(keys.map((k) => [k, null])));
  add(name, Object.fromEntries(keys.map((k) => [k, sample(props[k]!)])));
  for (let i = 0; i + 1 < keys.length; i++) {
    add(name, { ...valid, [keys[i + 1]!]: 5, [keys[i]!]: [] });
    add(name, { [keys[i + 1]!]: sample(props[keys[i + 1]!]!), ...Object.fromEntries(required.filter((k) => k !== keys[i]).map((k) => [k, 5])) });
  }
}

const out: { dto: string; body: unknown; details: string | null }[] = [];
for (const [dto, set] of cases) {
  for (const raw of set) out.push({ dto, body: JSON.parse(raw), details: await details(dto, JSON.stringify(expand(JSON.parse(raw)))) });
}
writeFileSync(process.argv[2] ?? 'typebox_details.json', `${JSON.stringify(out, null, 0).replaceAll('},{', '},\n{')}\n`);
console.log(`${out.length} cases`, Object.fromEntries([...cases].map(([k, v]) => [k, v.size])));

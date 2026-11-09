import type {
  IncidentEntity,
  IncidentReplyEntity,
} from '@features/incident/domain/entities/incident.entity';
import type {
  IncidentEvent,
  IncidentEventPublisher,
  IncidentRepository,
} from '@features/incident/domain/repositories/incident.repository';
import { createIncidentSseStream } from '@features/incident/presentation/incident-sse';
import { IncidentMapper } from '@features/incident/presentation/mappers/incident.mapper';
import { CreateIncidentUseCase } from '@features/incident/usecases/create-incident.usecase';
import { DeleteIncidentUseCase } from '@features/incident/usecases/delete-incident.usecase';
import { GetIncidentDetailUseCase } from '@features/incident/usecases/get-incident-detail.usecase';
import { PostIncidentReplyUseCase } from '@features/incident/usecases/post-incident-reply.usecase';
import { ResolveIncidentUseCase } from '@features/incident/usecases/resolve-incident.usecase';
import { describe, expect, it } from 'vitest';

const user = { email: 'a@x.id', name: 'Andi', role: 'GUARD', group_uid: 'g1', allow_admin: false };

/** In-memory repository: enough to test the business rules without a database. */
function makeRepo() {
  const incidents = new Map<string, IncidentEntity>();
  const replies: IncidentReplyEntity[] = [];
  let deleted = new Set<string>();
  const repo: IncidentRepository = {
    list: async (g) => [...incidents.values()].filter((i) => i.groupUid === g),
    findById: async (id, g) => {
      const i = incidents.get(id);
      return i && i.groupUid === g && !deleted.has(id) ? i : null;
    },
    listReplies: async (id) => replies.filter((r) => r.incidentId === id),
    create: async (d) => {
      const e: IncidentEntity = {
        ...d,
        status: 'OPEN',
        resolvedBy: null,
        resolvedByName: null,
        resolvedAt: null,
        createdAt: new Date(),
        updatedAt: new Date(),
      };
      incidents.set(d.id, e);
      return e;
    },
    addReply: async (d) => {
      const r = { ...d, createdAt: new Date() };
      replies.push(r);
      return r;
    },
    resolve: async (id, by) => {
      const i = incidents.get(id);
      if (!i) return null;
      if (i.status === 'OPEN') {
        Object.assign(i, {
          status: 'RESOLVED',
          resolvedBy: by.email,
          resolvedByName: by.name,
          resolvedAt: new Date(),
        });
      }
      return i;
    },
    softDelete: async (id) => {
      deleted.add(id);
      return new Date();
    },
  };
  const events: IncidentEvent[] = [];
  const publisher: IncidentEventPublisher = { publish: async (_id, e) => void events.push(e) };
  return { repo, publisher, events, reset: () => (deleted = new Set()) };
}

const newIncident = async (repo: IncidentRepository) =>
  (
    await new CreateIncidentUseCase(repo).execute({
      currentUser: user,
      data: { title: 'Pagar rusak', photos: [] },
    })
  ).value;

describe('incident usecases', () => {
  it('create: default kategori general, tolak kategori/koordinat tidak valid, grup dari token', async () => {
    const { repo } = makeRepo();
    const created = await newIncident(repo);
    expect(created.category).toBe('general');
    expect(created.groupUid).toBe('g1');
    expect(created.id).toMatch(/^inc_/);

    const uc = new CreateIncidentUseCase(repo);
    const badCat = await uc.execute({
      currentUser: user,
      data: { title: 't', category: 'ngawur', photos: [] },
    });
    const halfCoord = await uc.execute({
      currentUser: user,
      data: { title: 't', glat: 1, photos: [] },
    });
    expect(badCat.isFailure()).toBe(true);
    expect(halfCoord.isFailure()).toBe(true);
  });

  it('detail: insiden grup lain tidak ditemukan (isolasi tenant)', async () => {
    const { repo } = makeRepo();
    const inc = await newIncident(repo);
    const r = await new GetIncidentDetailUseCase(repo).execute({
      id: inc.id,
      currentUser: { group_uid: 'g2' },
    });
    expect(r.isFailure()).toBe(true);
    expect(r.failure.statusCode).toBe(404);
  });

  it('reply: publish new_reply; ditolak setelah resolved', async () => {
    const { repo, publisher, events } = makeRepo();
    const inc = await newIncident(repo);
    const reply = new PostIncidentReplyUseCase(repo, publisher);

    const ok = await reply.execute({
      id: inc.id,
      currentUser: user,
      data: { message: 'siap', photos: [] },
    });
    expect(ok.isSuccess()).toBe(true);
    expect(events.map((e) => e.type)).toEqual(['new_reply']);

    await new ResolveIncidentUseCase(repo, publisher).execute({ id: inc.id, currentUser: user });
    const closed = await reply.execute({
      id: inc.id,
      currentUser: user,
      data: { message: 'lagi', photos: [] },
    });
    expect(closed.isFailure()).toBe(true);

    const empty = await reply.execute({
      id: inc.id,
      currentUser: user,
      data: { message: '', photos: [] },
    });
    expect(empty.isFailure()).toBe(true);
  });

  it('resolve: idempoten, event thread_resolved hanya sekali', async () => {
    const { repo, publisher, events } = makeRepo();
    const inc = await newIncident(repo);
    const uc = new ResolveIncidentUseCase(repo, publisher);
    const first = await uc.execute({ id: inc.id, currentUser: user });
    const second = await uc.execute({ id: inc.id, currentUser: { ...user, email: 'b@x.id' } });
    expect(first.value.status).toBe('RESOLVED');
    expect(second.value.resolvedBy).toBe('a@x.id');
    expect(events.filter((e) => e.type === 'thread_resolved')).toHaveLength(1);
  });

  it('delete: hanya allow_admin; setelah hapus tidak ditemukan; publish incident_deleted', async () => {
    const { repo, publisher, events } = makeRepo();
    const inc = await newIncident(repo);
    const uc = new DeleteIncidentUseCase(repo, publisher);

    const denied = await uc.execute({ id: inc.id, currentUser: user });
    expect(denied.failure.statusCode).toBe(403);

    const admin = { ...user, allow_admin: true };
    expect((await uc.execute({ id: inc.id, currentUser: admin })).isSuccess()).toBe(true);
    expect(events.map((e) => e.type)).toEqual(['incident_deleted']);
    expect((await uc.execute({ id: inc.id, currentUser: admin })).failure.statusCode).toBe(404);
  });

  it('publish gagal tidak menggagalkan request', async () => {
    const { repo } = makeRepo();
    const inc = await newIncident(repo);
    const broken: IncidentEventPublisher = {
      publish: async () => {
        throw new Error('redis down');
      },
    };
    const r = await new PostIncidentReplyUseCase(repo, broken).execute({
      id: inc.id,
      currentUser: user,
      data: { message: 'halo', photos: [] },
    });
    expect(r.isSuccess()).toBe(true);
  });
});

describe('IncidentMapper.toSseFrame (kontrak dengan app Flutter)', () => {
  it('nama event + payload snake_case; tanggal string tetap diterima (round-trip Redis)', () => {
    const reply: IncidentReplyEntity = {
      id: 'rpl_1',
      incidentId: 'inc_1',
      senderEmail: 'a@x.id',
      senderName: 'Andi',
      senderRole: 'GUARD',
      message: 'm',
      photos: [],
      createdAt: new Date('2026-01-01T00:00:00Z'),
    };
    const viaRedis = JSON.parse(JSON.stringify({ type: 'new_reply', reply })) as IncidentEvent;
    const f = IncidentMapper.toSseFrame(viaRedis);
    expect(f).toMatchObject({ id: 'rpl_1', name: 'new_reply' });
    expect(f.data).toMatchObject({
      incident_id: 'inc_1',
      sender_id: 'a@x.id',
      created_at: '2026-01-01T00:00:00.000Z',
    });

    expect(
      IncidentMapper.toSseFrame({
        type: 'incident_deleted',
        incidentId: 'inc_1',
        deletedAt: new Date(0),
      }),
    ).toMatchObject({
      name: 'incident_deleted',
      data: { incident_id: 'inc_1', deleted_at: '1970-01-01T00:00:00.000Z' },
    });
  });
});

describe('createIncidentSseStream', () => {
  async function readAll(stream: ReadableStream<Uint8Array>, until: (s: string) => boolean) {
    const reader = stream.getReader();
    const dec = new TextDecoder();
    let out = '';
    while (!until(out)) {
      const { value, done } = await reader.read();
      if (done) break;
      out += dec.decode(value);
    }
    return { out, reader };
  }

  function makeSource() {
    let listener: ((e: IncidentEvent) => void) | undefined;
    let unsubscribed = 0;
    return {
      source: {
        subscribe: async (_id: string, l: (e: IncidentEvent) => void) => {
          listener = l;
          return async () => void unsubscribed++;
        },
      },
      emit: (e: IncidentEvent) => listener?.(e),
      unsubscribed: () => unsubscribed,
    };
  }

  it('baru terhubung: retry saja; teruskan event; unsubscribe saat cancel', async () => {
    const s = makeSource();
    const stream = createIncidentSseStream({
      incidentId: 'inc_1',
      source: s.source,
      heartbeatMs: 10_000,
    });
    const reader = stream.getReader();
    const dec = new TextDecoder();
    expect(dec.decode((await reader.read()).value)).toBe('retry: 3000\n\n');

    s.emit({ type: 'incident_deleted', incidentId: 'inc_1', deletedAt: new Date(0) });
    const frame = dec.decode((await reader.read()).value);
    expect(frame).toContain('event: incident_deleted\n');
    expect(frame).toContain('"incident_id":"inc_1"');
    expect(frame.endsWith('\n\n')).toBe(true);

    await reader.cancel();
    expect(s.unsubscribed()).toBe(1);
  });

  it('reconnect dengan Last-Event-ID -> sync_required; heartbeat terkirim', async () => {
    const s = makeSource();
    const stream = createIncidentSseStream({
      incidentId: 'inc_1',
      lastEventId: 'rpl_9',
      source: s.source,
      heartbeatMs: 5,
    });
    const { out, reader } = await readAll(stream, (o) => o.includes(': ping'));
    expect(out).toContain('event: sync_required\ndata: {}\n\n');
    await reader.cancel();
  });

  it('abort signal -> unsubscribe & stream selesai', async () => {
    const s = makeSource();
    const ac = new AbortController();
    const stream = createIncidentSseStream({
      incidentId: 'inc_1',
      source: s.source,
      signal: ac.signal,
    });
    const reader = stream.getReader();
    await reader.read();
    ac.abort();
    expect((await reader.read()).done).toBe(true);
    expect(s.unsubscribed()).toBe(1);
  });
});

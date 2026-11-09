import type { AuthenticatedHandlerContext } from '@core/types/context.types';
import { ApiResponse } from '@core/utils/api-response';
import type {
  CreateIncidentDtoType,
  GetIncidentsQueryDtoType,
  IncidentIdParamsDtoType,
  PostIncidentReplyDtoType,
} from '@features/incident/presentation/dtos/incident-request.dto';
import {
  createIncidentSseStream,
  type IncidentEventSource,
} from '@features/incident/presentation/incident-sse';
import { IncidentMapper } from '@features/incident/presentation/mappers/incident.mapper';
import type { CreateIncidentUseCase } from '@features/incident/usecases/create-incident.usecase';
import type { DeleteIncidentUseCase } from '@features/incident/usecases/delete-incident.usecase';
import type { GetIncidentDetailUseCase } from '@features/incident/usecases/get-incident-detail.usecase';
import type { GetIncidentsUseCase } from '@features/incident/usecases/get-incidents.usecase';
import type { PostIncidentReplyUseCase } from '@features/incident/usecases/post-incident-reply.usecase';
import type { ResolveIncidentUseCase } from '@features/incident/usecases/resolve-incident.usecase';

type Ctx = AuthenticatedHandlerContext;
type WithParams = Ctx & { params: IncidentIdParamsDtoType };

export class IncidentHandler {
  constructor(
    private readonly getIncidents: GetIncidentsUseCase,
    private readonly getIncidentDetail: GetIncidentDetailUseCase,
    private readonly createIncident: CreateIncidentUseCase,
    private readonly postReply: PostIncidentReplyUseCase,
    private readonly resolveIncident: ResolveIncidentUseCase,
    private readonly deleteIncident: DeleteIncidentUseCase,
    private readonly eventSource: IncidentEventSource,
  ) {}

  public async list({ query, currentUser, set }: Ctx & { query: GetIncidentsQueryDtoType }) {
    const result = await this.getIncidents.execute(
      IncidentMapper.toGetIncidentsInput(query, currentUser),
    );
    if (result.isFailure()) return this.fail(set, result.failure);
    return ApiResponse.success(
      result.value.map(IncidentMapper.toIncidentResponse),
      'Daftar insiden berhasil diambil',
    );
  }

  public async detail({ params, currentUser, set }: WithParams) {
    const result = await this.getIncidentDetail.execute({ id: params.id, currentUser });
    if (result.isFailure()) return this.fail(set, result.failure);
    return ApiResponse.success(
      IncidentMapper.toDetailResponse(result.value),
      'Detail insiden berhasil diambil',
    );
  }

  public async create({ body, currentUser, set }: Ctx & { body: CreateIncidentDtoType }) {
    const result = await this.createIncident.execute(
      IncidentMapper.toCreateInput(body, currentUser),
    );
    if (result.isFailure()) return this.fail(set, result.failure);
    set.status = 'Created';
    return ApiResponse.success(
      IncidentMapper.toIncidentResponse(result.value),
      'Insiden berhasil dilaporkan',
    );
  }

  public async reply({
    params,
    body,
    currentUser,
    set,
  }: WithParams & { body: PostIncidentReplyDtoType }) {
    const result = await this.postReply.execute({
      id: params.id,
      currentUser,
      data: IncidentMapper.toReplyData(body),
    });
    if (result.isFailure()) return this.fail(set, result.failure);
    set.status = 'Created';
    return ApiResponse.success(
      IncidentMapper.toReplyResponse(result.value),
      'Balasan berhasil dikirim',
    );
  }

  public async resolve({ params, currentUser, set }: WithParams) {
    const result = await this.resolveIncident.execute({ id: params.id, currentUser });
    if (result.isFailure()) return this.fail(set, result.failure);
    return ApiResponse.success(
      IncidentMapper.toIncidentResponse(result.value),
      'Insiden berhasil diselesaikan',
    );
  }

  public async remove({ params, currentUser, set }: WithParams) {
    const result = await this.deleteIncident.execute({ id: params.id, currentUser });
    if (result.isFailure()) return this.fail(set, result.failure);
    return ApiResponse.success(null, 'Insiden berhasil dihapus');
  }

  /** Checks access (group + existence) before opening the stream; a failure is a plain JSON error, not SSE. */
  public async stream({ params, currentUser, set, request }: WithParams & { request: Request }) {
    const check = await this.getIncidentDetail.execute({ id: params.id, currentUser });
    if (check.isFailure()) return this.fail(set, check.failure);

    const body = createIncidentSseStream({
      incidentId: check.value.incident.id,
      lastEventId: request.headers.get('last-event-id') ?? undefined,
      source: this.eventSource,
      signal: request.signal,
    });
    return new Response(body, {
      headers: {
        'Content-Type': 'text/event-stream; charset=utf-8',
        'Cache-Control': 'no-cache, no-transform',
        Connection: 'keep-alive',
        'X-Accel-Buffering': 'no',
      },
    });
  }

  private fail(set: Ctx['set'], failure: Parameters<typeof ApiResponse.fromFailure>[0]) {
    set.status = failure.status;
    return ApiResponse.fromFailure(failure);
  }
}

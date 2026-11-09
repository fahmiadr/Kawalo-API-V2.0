import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiCreatedResponseSchema, ApiResponseSchema } from '@core/utils/api-response';
import { incidentHandler } from '@features/incident/incident.module';
import {
  CreateIncidentDto,
  GetIncidentsQueryDto,
  IncidentIdParamsDto,
  PostIncidentReplyDto,
} from '@features/incident/presentation/dtos/incident-request.dto';
import {
  IncidentDetailResponseDto,
  IncidentItemDto,
  IncidentListResponseDto,
  IncidentReplyItemDto,
} from '@features/incident/presentation/dtos/incident-response.dto';
import { Elysia, t } from 'elysia';

export const incidentRoutes = new Elysia({ prefix: '/incidents' })
  .use(authMiddleware)
  .get(
    '/',
    ({ query, currentUser, set }) =>
      incidentHandler.list({ query, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      query: GetIncidentsQueryDto,
      response: ApiResponseSchema(IncidentListResponseDto),
    },
  )
  .post(
    '/',
    ({ body, currentUser, set }) =>
      incidentHandler.create({ body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      body: CreateIncidentDto,
      response: ApiCreatedResponseSchema(IncidentItemDto),
    },
  )
  .get(
    '/:id',
    ({ params, currentUser, set }) =>
      incidentHandler.detail({ params, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      params: IncidentIdParamsDto,
      response: ApiResponseSchema(IncidentDetailResponseDto),
    },
  )
  // SSE: tanpa skema response (body berupa stream)
  .get(
    '/:id/stream',
    ({ params, currentUser, set, request }) =>
      incidentHandler.stream({ params, currentUser: currentUser!, set, request }),
    { requireAuth: true, params: IncidentIdParamsDto },
  )
  .post(
    '/:id/replies',
    ({ params, body, currentUser, set }) =>
      incidentHandler.reply({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      params: IncidentIdParamsDto,
      body: PostIncidentReplyDto,
      response: ApiCreatedResponseSchema(IncidentReplyItemDto),
    },
  )
  .patch(
    '/:id/resolve',
    ({ params, currentUser, set }) =>
      incidentHandler.resolve({ params, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      params: IncidentIdParamsDto,
      response: ApiResponseSchema(IncidentItemDto),
    },
  )
  .delete(
    '/:id',
    ({ params, currentUser, set }) =>
      incidentHandler.remove({ params, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      params: IncidentIdParamsDto,
      response: ApiResponseSchema(t.Null()),
    },
  );

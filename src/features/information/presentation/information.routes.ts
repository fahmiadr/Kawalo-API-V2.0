import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiCreatedResponseSchema, ApiResponseSchema } from '@core/utils/api-response';
import { informationHandler } from '@features/information/information.module';
import {
  CreateInformationDto,
  DeleteBatchInformationDto,
  GetInformationQueryDto,
  InformationIdParamsDto,
  UpdateInformationDto,
} from '@features/information/presentation/dtos/information-request.dto';
import {
  DeleteBatchInformationResponseDto,
  GetInformationResponseDto,
  InformationItemDto,
} from '@features/information/presentation/dtos/information-response.dto';
import { Elysia, t } from 'elysia';

export const informationRoutes = new Elysia({ prefix: '/information' })
  .use(authMiddleware)
  .get(
    '/',
    ({ query, currentUser, set }) =>
      informationHandler.getInformation({ query, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      query: GetInformationQueryDto,
      response: ApiResponseSchema(GetInformationResponseDto),
    },
  )
  .post(
    '/',
    ({ body, currentUser, set }) =>
      informationHandler.createInformation({ body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      body: CreateInformationDto,
      response: ApiCreatedResponseSchema(InformationItemDto),
    },
  )
  .put(
    '/:id',
    ({ params, body, currentUser, set }) =>
      informationHandler.updateInformation({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: InformationIdParamsDto,
      body: UpdateInformationDto,
      response: ApiResponseSchema(InformationItemDto),
    },
  )
  .patch(
    '/:id',
    ({ params, body, currentUser, set }) =>
      informationHandler.updateInformation({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: InformationIdParamsDto,
      body: UpdateInformationDto,
      response: ApiResponseSchema(InformationItemDto),
    },
  )
  .delete(
    '/',
    ({ body, currentUser, set }) =>
      informationHandler.deleteBatchInformation({ body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      body: DeleteBatchInformationDto,
      response: ApiResponseSchema(DeleteBatchInformationResponseDto),
    },
  )
  .delete(
    '/:id',
    ({ params, currentUser, set }) =>
      informationHandler.deleteInformation({ params, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: InformationIdParamsDto,
      response: ApiResponseSchema(t.Null()),
    },
  );

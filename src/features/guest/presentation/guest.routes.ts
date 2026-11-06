import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiCreatedResponseSchema, ApiResponseSchema } from '@core/utils/api-response';
import { guestHandler } from '@features/guest/guest.module';
import {
  AcknowledgeGuestDto,
  CheckoutGuestDto,
  CreateGuestDto,
  DeleteBatchGuestDto,
  GetGuestsQueryDto,
  GuestIdParamsDto,
  UpdateGuestDto,
} from '@features/guest/presentation/dtos/guest-request.dto';
import {
  DeleteBatchGuestResponseDto,
  GetGuestsResponseDto,
  GetMasterGuestTypesResponseDto,
  GuestItemDto,
} from '@features/guest/presentation/dtos/guest-response.dto';
import { Elysia, t } from 'elysia';

/**
 * `requireAuth: true` pada authMiddleware menjamin `currentUser` non-null sebelum
 * handler dijalankan. Non-null assertion (`!`) digunakan karena Elysia derive
 * mengetik `currentUser` sebagai `JwtUserPayload | null` secara statis.
 */
export const guestRoutes = new Elysia({ prefix: '/guests' })
  .use(authMiddleware)
  .get(
    '/master-types',
    ({ currentUser, set }) => guestHandler.getMasterGuestTypes({ currentUser: currentUser!, set }),
    {
      requireAuth: true,
      response: ApiResponseSchema(GetMasterGuestTypesResponseDto),
    },
  )
  .get(
    '/',
    ({ query, currentUser, set }) =>
      guestHandler.getGuests({ query, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      query: GetGuestsQueryDto,
      response: ApiResponseSchema(GetGuestsResponseDto),
    },
  )
  .get(
    '/:id',
    ({ params, currentUser, set }) =>
      guestHandler.getGuestById({ params, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      params: GuestIdParamsDto,
      response: ApiResponseSchema(t.Nullable(GuestItemDto)),
    },
  )
  .post(
    '/',
    ({ body, currentUser, set }) =>
      guestHandler.createGuest({ body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      body: CreateGuestDto,
      response: ApiCreatedResponseSchema(GuestItemDto),
    },
  )
  .patch(
    '/:id/acknowledge',
    ({ params, body, currentUser, set }) =>
      guestHandler.acknowledgeGuest({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      params: GuestIdParamsDto,
      body: AcknowledgeGuestDto,
      response: ApiResponseSchema(GuestItemDto),
    },
  )
  .patch(
    '/:id/checkout',
    ({ params, body, currentUser, set }) =>
      guestHandler.checkoutGuest({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: GuestIdParamsDto,
      body: CheckoutGuestDto,
      response: ApiResponseSchema(GuestItemDto),
    },
  )
  .put(
    '/:id',
    ({ params, body, currentUser, set }) =>
      guestHandler.updateGuest({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: GuestIdParamsDto,
      body: UpdateGuestDto,
      response: ApiResponseSchema(GuestItemDto),
    },
  )
  .patch(
    '/:id',
    ({ params, body, currentUser, set }) =>
      guestHandler.updateGuest({ params, body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: GuestIdParamsDto,
      body: UpdateGuestDto,
      response: ApiResponseSchema(GuestItemDto),
    },
  )
  .delete(
    '/',
    ({ body, currentUser, set }) =>
      guestHandler.deleteBatchGuest({ body, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      body: DeleteBatchGuestDto,
      response: ApiResponseSchema(DeleteBatchGuestResponseDto),
    },
  )
  .delete(
    '/:id',
    ({ params, currentUser, set }) =>
      guestHandler.deleteGuest({ params, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      requireRole: ['ADMIN', 'GUARD'],
      params: GuestIdParamsDto,
      response: ApiResponseSchema(t.Null()),
    },
  );

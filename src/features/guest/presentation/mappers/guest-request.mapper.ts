import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import {
  cleanQueryBoolean,
  cleanQueryDate,
  cleanQueryString,
  cleanUpdateString,
} from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import type {
  AcknowledgeGuestDtoType,
  CheckoutGuestDtoType,
  CreateGuestDtoType,
  DeleteBatchGuestDtoType,
  GetGuestsQueryDtoType,
  GuestIdParamsDtoType,
  UpdateGuestDtoType,
} from '@features/guest/presentation/dtos/guest-request.dto';
import type { AcknowledgeGuestInput } from '@features/guest/usecases/acknowledge-guest.usecase';
import type { CheckoutGuestInput } from '@features/guest/usecases/checkout-guest.usecase';
import type { CreateGuestInput } from '@features/guest/usecases/create-guest.usecase';
import type { DeleteGuestInput } from '@features/guest/usecases/delete-guest.usecase';
import type { GetGuestByIdInput } from '@features/guest/usecases/get-guest-by-id.usecase';
import type { GetGuestsInput } from '@features/guest/usecases/get-guests.usecase';
import type { GetMasterGuestTypesInput } from '@features/guest/usecases/get-master-guest-types.usecase';
import type { UpdateGuestInput } from '@features/guest/usecases/update-guest.usecase';

export class GuestRequestMapper {
  public static toGetGuestByIdInput(
    params: GuestIdParamsDtoType,
    currentUser: AuthenticatedUserPayload,
  ): GetGuestByIdInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
    };
  }
  public static toGetGuestsInput(
    query: GetGuestsQueryDtoType,
    currentUser: AuthenticatedUserPayload,
  ): GetGuestsInput {
    const { take, skip } = resolvePagination({
      limit: query.limit,
      offset: query.offset,
    });

    return {
      currentUser,
      search: cleanQueryString(query.search),
      limit: take,
      offset: skip,
      startDate: cleanQueryDate(query.start_date),
      endDate: cleanQueryDate(query.end_date),
      createdBy: cleanQueryString(query.created_by),
      isActive: cleanQueryBoolean(query.is_active),
    };
  }

  public static toGetMasterGuestTypesInput(
    currentUser: AuthenticatedUserPayload,
  ): GetMasterGuestTypesInput {
    return {
      currentUser,
    };
  }

  public static toCreateGuestInput(
    body: CreateGuestDtoType,
    currentUser: AuthenticatedUserPayload,
  ): CreateGuestInput {
    return {
      currentUser,
      data: {
        guestName: cleanQueryString(body.guest_name),
        guestDestination: cleanQueryString(body.guest_destination),
        guestPurpose: cleanQueryString(body.guest_purpose),
        guestNumber: cleanQueryString(body.guest_number),
        guestIdentity: cleanQueryString(body.guest_identity),
        guestIdentityType: cleanQueryString(body.guest_identity_type),
        guestType: cleanQueryString(body.guest_type),
        guestAddress: cleanQueryString(body.guest_address),
        guestToUserEmail: cleanQueryString(body.guest_to_user_email),
        guestPhoto: cleanQueryString(body.guest_photo),
        guestDt: cleanQueryDate(body.guest_dt),
      },
    };
  }

  public static toUpdateGuestInput(
    params: GuestIdParamsDtoType,
    body: UpdateGuestDtoType,
    currentUser: AuthenticatedUserPayload,
  ): UpdateGuestInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
      data: {
        guestName: cleanUpdateString(body.guest_name),
        guestDestination: cleanUpdateString(body.guest_destination),
        guestPurpose: cleanUpdateString(body.guest_purpose),
        guestNumber: cleanUpdateString(body.guest_number),
        guestIdentity: cleanUpdateString(body.guest_identity),
        guestIdentityType: cleanUpdateString(body.guest_identity_type),
        guestType: cleanUpdateString(body.guest_type),
        guestAddress: cleanUpdateString(body.guest_address),
        guestToUserEmail: cleanUpdateString(body.guest_to_user_email),
        guestPhoto: cleanUpdateString(body.guest_photo),
      },
    };
  }

  public static toAcknowledgeGuestInput(
    params: GuestIdParamsDtoType,
    body: AcknowledgeGuestDtoType,
    currentUser: AuthenticatedUserPayload,
  ): AcknowledgeGuestInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
      data: {
        qrUid: cleanQueryString(body.qr_uid),
      },
    };
  }

  public static toCheckoutGuestInput(
    params: GuestIdParamsDtoType,
    body: CheckoutGuestDtoType,
    currentUser: AuthenticatedUserPayload,
  ): CheckoutGuestInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
      data: {
        qrUid: cleanQueryString(body.qr_uid),
      },
    };
  }

  public static toDeleteGuestInput(
    params: GuestIdParamsDtoType,
    currentUser: AuthenticatedUserPayload,
  ): DeleteGuestInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
    };
  }

  public static toDeleteBatchGuestInput(
    body: DeleteBatchGuestDtoType,
    currentUser: AuthenticatedUserPayload,
  ): DeleteGuestInput {
    const ids = body.ids
      .map((id) => cleanQueryString(id))
      .filter((id): id is string => Boolean(id));

    return {
      ids,
      currentUser,
    };
  }
}

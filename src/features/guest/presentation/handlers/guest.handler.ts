import type { AuthenticatedHandlerContext } from '@core/types/context.types';
import { ApiResponse } from '@core/utils/api-response';
import type {
  AcknowledgeGuestDtoType,
  CheckoutGuestDtoType,
  CreateGuestDtoType,
  DeleteBatchGuestDtoType,
  GetGuestsQueryDtoType,
  GuestIdParamsDtoType,
  UpdateGuestDtoType,
} from '@features/guest/presentation/dtos/guest-request.dto';
import { GuestRequestMapper } from '@features/guest/presentation/mappers/guest-request.mapper';
import { GuestResponseMapper } from '@features/guest/presentation/mappers/guest-response.mapper';
import type { AcknowledgeGuestUseCase } from '@features/guest/usecases/acknowledge-guest.usecase';
import type { CheckoutGuestUseCase } from '@features/guest/usecases/checkout-guest.usecase';
import type { CreateGuestUseCase } from '@features/guest/usecases/create-guest.usecase';
import type { DeleteGuestUseCase } from '@features/guest/usecases/delete-guest.usecase';
import type { GetGuestByIdUseCase } from '@features/guest/usecases/get-guest-by-id.usecase';
import type { GetGuestsUseCase } from '@features/guest/usecases/get-guests.usecase';
import type { GetMasterGuestTypesUseCase } from '@features/guest/usecases/get-master-guest-types.usecase';
import type { UpdateGuestUseCase } from '@features/guest/usecases/update-guest.usecase';

export interface GetGuestsContext extends AuthenticatedHandlerContext {
  query: GetGuestsQueryDtoType;
}

export interface GetGuestByIdContext extends AuthenticatedHandlerContext {
  params: GuestIdParamsDtoType;
}

export interface CreateGuestContext extends AuthenticatedHandlerContext {
  body: CreateGuestDtoType;
}

export interface UpdateGuestContext extends AuthenticatedHandlerContext {
  params: GuestIdParamsDtoType;
  body: UpdateGuestDtoType;
}

export interface DeleteGuestContext extends AuthenticatedHandlerContext {
  params: GuestIdParamsDtoType;
}

export interface DeleteBatchGuestContext extends AuthenticatedHandlerContext {
  body: DeleteBatchGuestDtoType;
}

export interface AcknowledgeGuestContext extends AuthenticatedHandlerContext {
  params: GuestIdParamsDtoType;
  body: AcknowledgeGuestDtoType;
}

export interface CheckoutGuestContext extends AuthenticatedHandlerContext {
  params: GuestIdParamsDtoType;
  body: CheckoutGuestDtoType;
}

export interface GetMasterGuestTypesContext extends AuthenticatedHandlerContext {}

export class GuestHandler {
  constructor(
    private readonly getGuestsUseCase: GetGuestsUseCase,
    private readonly getGuestByIdUseCase: GetGuestByIdUseCase,
    private readonly createGuestUseCase: CreateGuestUseCase,
    private readonly updateGuestUseCase: UpdateGuestUseCase,
    private readonly deleteGuestUseCase: DeleteGuestUseCase,
    private readonly acknowledgeGuestUseCase: AcknowledgeGuestUseCase,
    private readonly checkoutGuestUseCase: CheckoutGuestUseCase,
    private readonly getMasterGuestTypesUseCase: GetMasterGuestTypesUseCase,
  ) {}

  public async getGuests({ query, currentUser, set }: GetGuestsContext) {
    const input = GuestRequestMapper.toGetGuestsInput(query, currentUser);
    const result = await this.getGuestsUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GuestResponseMapper.toResponseList(result.value);
    return ApiResponse.success(responseData, 'Daftar data tamu berhasil diambil');
  }

  public async getGuestById({ params, currentUser, set }: GetGuestByIdContext) {
    const input = GuestRequestMapper.toGetGuestByIdInput(params, currentUser);
    const result = await this.getGuestByIdUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = result.value ? GuestResponseMapper.toResponse(result.value) : null;
    return ApiResponse.success(responseData, 'Data tamu berhasil diambil');
  }

  public async createGuest({ body, currentUser, set }: CreateGuestContext) {
    const input = GuestRequestMapper.toCreateGuestInput(body, currentUser);
    const result = await this.createGuestUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    set.status = 'Created';
    const responseData = GuestResponseMapper.toResponse(result.value);
    return ApiResponse.success(responseData, 'Data tamu berhasil dibuat');
  }

  public async updateGuest({ params, body, currentUser, set }: UpdateGuestContext) {
    const input = GuestRequestMapper.toUpdateGuestInput(params, body, currentUser);
    const result = await this.updateGuestUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GuestResponseMapper.toResponse(result.value);
    return ApiResponse.success(responseData, 'Data tamu berhasil diperbarui');
  }

  public async deleteGuest({ params, currentUser, set }: DeleteGuestContext) {
    const input = GuestRequestMapper.toDeleteGuestInput(params, currentUser);
    const result = await this.deleteGuestUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    return ApiResponse.success(null, 'Data tamu berhasil dihapus');
  }

  public async deleteBatchGuest({ body, currentUser, set }: DeleteBatchGuestContext) {
    const input = GuestRequestMapper.toDeleteBatchGuestInput(body, currentUser);
    const result = await this.deleteGuestUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GuestResponseMapper.toDeleteBatchResponse(result.value);
    return ApiResponse.success(
      responseData,
      `${result.value.deletedCount} data tamu berhasil dihapus`,
    );
  }

  public async acknowledgeGuest({ params, body, currentUser, set }: AcknowledgeGuestContext) {
    const input = GuestRequestMapper.toAcknowledgeGuestInput(params, body, currentUser);
    const result = await this.acknowledgeGuestUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GuestResponseMapper.toResponse(result.value);
    return ApiResponse.success(responseData, 'Data tamu berhasil dikonfirmasi');
  }

  public async checkoutGuest({ params, body, currentUser, set }: CheckoutGuestContext) {
    const input = GuestRequestMapper.toCheckoutGuestInput(params, body, currentUser);
    const result = await this.checkoutGuestUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GuestResponseMapper.toResponse(result.value);
    return ApiResponse.success(responseData, 'Tamu berhasil check-out');
  }

  public async getMasterGuestTypes({ currentUser, set }: GetMasterGuestTypesContext) {
    const input = GuestRequestMapper.toGetMasterGuestTypesInput(currentUser);
    const result = await this.getMasterGuestTypesUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GuestResponseMapper.toMasterGuestTypeList(result.value);
    return ApiResponse.success(responseData, 'Daftar tipe tamu master berhasil diambil');
  }
}

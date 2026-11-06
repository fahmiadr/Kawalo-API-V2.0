import type { AuthenticatedHandlerContext } from '@core/types/context.types';
import { ApiResponse } from '@core/utils/api-response';
import type {
  CreateInformationDtoType,
  DeleteBatchInformationDtoType,
  GetInformationQueryDtoType,
  InformationIdParamsDtoType,
  UpdateInformationDtoType,
} from '@features/information/presentation/dtos/information-request.dto';
import { InformationRequestMapper } from '@features/information/presentation/mappers/information-request.mapper';
import { InformationResponseMapper } from '@features/information/presentation/mappers/information-response.mapper';
import type { CreateInformationUseCase } from '@features/information/usecases/create-information.usecase';
import type { DeleteInformationUseCase } from '@features/information/usecases/delete-information.usecase';
import type { GetInformationUseCase } from '@features/information/usecases/get-information.usecase';
import type { UpdateInformationUseCase } from '@features/information/usecases/update-information.usecase';

export interface GetInformationContext extends AuthenticatedHandlerContext {
  query: GetInformationQueryDtoType;
}

export interface CreateInformationContext extends AuthenticatedHandlerContext {
  body: CreateInformationDtoType;
}

export interface UpdateInformationContext extends AuthenticatedHandlerContext {
  params: InformationIdParamsDtoType;
  body: UpdateInformationDtoType;
}

export interface DeleteInformationContext extends AuthenticatedHandlerContext {
  params: InformationIdParamsDtoType;
}

export interface DeleteBatchInformationContext extends AuthenticatedHandlerContext {
  body: DeleteBatchInformationDtoType;
}

export class InformationHandler {
  constructor(
    private readonly getInformationUseCase: GetInformationUseCase,
    private readonly createInformationUseCase: CreateInformationUseCase,
    private readonly updateInformationUseCase: UpdateInformationUseCase,
    private readonly deleteInformationUseCase: DeleteInformationUseCase,
  ) {}

  public async getInformation({ query, currentUser, set }: GetInformationContext) {
    const input = InformationRequestMapper.toGetInformationInput(query, currentUser);
    const result = await this.getInformationUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = InformationResponseMapper.toResponseList(result.value);
    return ApiResponse.success(responseData, 'Daftar data informasi berhasil diambil');
  }

  public async createInformation({ body, currentUser, set }: CreateInformationContext) {
    const input = InformationRequestMapper.toCreateInformationInput(body, currentUser);
    const result = await this.createInformationUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    set.status = 'Created';
    const responseData = InformationResponseMapper.toResponse(result.value);
    return ApiResponse.success(responseData, 'Data informasi berhasil dibuat');
  }

  public async updateInformation({ params, body, currentUser, set }: UpdateInformationContext) {
    const input = InformationRequestMapper.toUpdateInformationInput(params, body, currentUser);
    const result = await this.updateInformationUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = InformationResponseMapper.toResponse(result.value);
    return ApiResponse.success(responseData, 'Data informasi berhasil diperbarui');
  }

  public async deleteInformation({ params, currentUser, set }: DeleteInformationContext) {
    const input = InformationRequestMapper.toDeleteInformationInput(params, currentUser);
    const result = await this.deleteInformationUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    return ApiResponse.success(null, 'Data informasi berhasil dihapus');
  }

  public async deleteBatchInformation({ body, currentUser, set }: DeleteBatchInformationContext) {
    const input = InformationRequestMapper.toDeleteBatchInformationInput(body, currentUser);
    const result = await this.deleteInformationUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = InformationResponseMapper.toDeleteBatchResponse(result.value);
    return ApiResponse.success(
      responseData,
      `${result.value.deletedCount} data informasi berhasil dihapus`,
    );
  }
}

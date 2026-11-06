import type { AuthenticatedHandlerContext } from '@core/types/context.types';
import { ApiResponse } from '@core/utils/api-response';
import type { GetOperationalQueryDtoType } from '@features/operational/presentation/dtos/operational-query.dto';
import { OperationalRequestMapper } from '@features/operational/presentation/mappers/operational-request.mapper';
import { OperationalResponseMapper } from '@features/operational/presentation/mappers/operational-response.mapper';
import type { GetOperationalDataUseCase } from '@features/operational/usecases/get-operational-data.usecase';

export interface GetOperationalContext extends AuthenticatedHandlerContext {
  query: GetOperationalQueryDtoType;
}

export class OperationalHandler {
  constructor(private readonly getOperationalDataUseCase: GetOperationalDataUseCase) {}

  public async getOperationals({ query, currentUser, set }: GetOperationalContext) {
    const input = OperationalRequestMapper.toGetOperationalInput(query, currentUser);
    const result = await this.getOperationalDataUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = OperationalResponseMapper.toResponseList(result.value);
    return ApiResponse.success(responseData, 'Daftar aktifitas operasional berhasil diambil');
  }
}

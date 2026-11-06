import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { cleanQueryDate, cleanQueryString } from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import type { GetOperationalQueryDtoType } from '@features/operational/presentation/dtos/operational-query.dto';
import type { GetOperationalInput } from '@features/operational/usecases/get-operational-data.usecase';

export class OperationalRequestMapper {
  public static toGetOperationalInput(
    query: GetOperationalQueryDtoType,
    currentUser: AuthenticatedUserPayload,
  ): GetOperationalInput {
    const { take, skip } = resolvePagination({
      limit: query.limit,
      offset: query.offset,
    });

    return {
      search: cleanQueryString(query.search),
      limit: take,
      offset: skip,
      startDate: cleanQueryDate(query.start_date),
      endDate: cleanQueryDate(query.end_date),
      currentUser,
    };
  }
}

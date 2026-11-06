import {
  cleanQueryCoordinate,
  cleanQueryDate,
  cleanQueryString,
  cleanString,
  cleanUpdateCoordinate,
  cleanUpdateString,
} from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import type {
  CreateInformationDtoType,
  DeleteBatchInformationDtoType,
  GetInformationQueryDtoType,
  InformationIdParamsDtoType,
  UpdateInformationDtoType,
} from '@features/information/presentation/dtos/information-request.dto';
import type { CreateInformationInput } from '@features/information/usecases/create-information.usecase';
import type { DeleteInformationInput } from '@features/information/usecases/delete-information.usecase';
import type { GetInformationInput } from '@features/information/usecases/get-information.usecase';
import type { UpdateInformationInput } from '@features/information/usecases/update-information.usecase';

export class InformationRequestMapper {
  public static toGetInformationInput(
    query: GetInformationQueryDtoType,
    currentUser: AuthenticatedUserPayload,
  ): GetInformationInput {
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
    };
  }

  /**
   * Membersihkan dan menggeser (auto-slide) elemen foto ke slot kiri.
   * Menjamin foto valid berurutan dari indeks 0 tanpa celah kosong di awal atau di antara,
   * sehingga bagian kiri terisi rapat dan bagian kanan boleh kosong (maksimal 4 foto).
   *
   * Contoh: ['', 'url1', '  ', 'url2'] -> ['url1', 'url2']
   */
  public static cleanAndSlidePhotos(
    photos?: (string | null | undefined)[] | null,
  ): string[] | undefined {
    if (photos === undefined) {
      return undefined;
    }
    if (photos === null || !Array.isArray(photos) || photos.length === 0) {
      return [];
    }

    const slided = photos
      .map((p) => cleanString(p))
      .filter((p): p is string => Boolean(p))
      .slice(0, 4);

    return slided;
  }

  public static toCreateInformationInput(
    body: CreateInformationDtoType,
    currentUser: AuthenticatedUserPayload,
  ): CreateInformationInput {
    const photos = InformationRequestMapper.cleanAndSlidePhotos(body.photos);
    return {
      currentUser,
      data: {
        title: cleanQueryString(body.title),
        text: cleanQueryString(body.text),
        feedType: cleanQueryString(body.feed_type),
        feedFile: cleanQueryString(body.feed_file),
        photos: photos !== undefined && photos.length > 0 ? photos : undefined,
        eventLocation: cleanQueryString(body.event_location),
        glat: cleanQueryCoordinate(body.glat),
        glong: cleanQueryCoordinate(body.glong),
      },
    };
  }

  public static toUpdateInformationInput(
    params: InformationIdParamsDtoType,
    body: UpdateInformationDtoType,
    currentUser: AuthenticatedUserPayload,
  ): UpdateInformationInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
      data: {
        title: cleanUpdateString(body.title),
        text: cleanUpdateString(body.text),
        feedType: cleanUpdateString(body.feed_type),
        feedFile: cleanUpdateString(body.feed_file),
        photos: InformationRequestMapper.cleanAndSlidePhotos(body.photos),
        eventLocation: cleanUpdateString(body.event_location),
        glat: cleanUpdateCoordinate(body.glat),
        glong: cleanUpdateCoordinate(body.glong),
      },
    };
  }

  public static toDeleteInformationInput(
    params: InformationIdParamsDtoType,
    currentUser: AuthenticatedUserPayload,
  ): DeleteInformationInput {
    return {
      id: cleanQueryString(params.id),
      currentUser,
    };
  }

  public static toDeleteBatchInformationInput(
    body: DeleteBatchInformationDtoType,
    currentUser: AuthenticatedUserPayload,
  ): DeleteInformationInput {
    const ids = body.ids
      .map((id) => cleanQueryString(id))
      .filter((id): id is string => Boolean(id));

    return {
      ids,
      currentUser,
    };
  }
}

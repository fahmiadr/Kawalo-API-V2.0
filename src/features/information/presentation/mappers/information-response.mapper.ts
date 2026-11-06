import type { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  DeleteBatchInformationResponseDtoType,
  InformationItemDtoType,
} from '@features/information/presentation/dtos/information-response.dto';
import type { DeleteInformationOutput } from '@features/information/usecases/delete-information.usecase';

export class InformationResponseMapper {
  public static toResponse(information: InformationEntity): InformationItemDtoType {
    return {
      id: information.id,
      title: information.title,
      text: information.text,
      user_email: information.userEmail,
      user_name: information.userName,
      user_profile_pic: information.userProfilePic,
      feed_dt: information.feedDt,
      feed_file: information.feedFile,
      photos: information.photos,
      feed_type: information.feedType,
      group_name: information.groupName,
      event_location: information.eventLocation,
      event_start_dt: information.eventStartDt,
      event_end_dt: information.eventEndDt,
      event_duration: information.eventDuration,
      glat: information.glat,
      glong: information.glong,
    };
  }

  public static toResponseList(informations: InformationEntity[]): InformationItemDtoType[] {
    return informations.map(InformationResponseMapper.toResponse);
  }

  public static toDeleteBatchResponse(
    output: DeleteInformationOutput,
  ): DeleteBatchInformationResponseDtoType {
    return {
      deleted_count: output.deletedCount,
    };
  }
}

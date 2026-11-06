import type {
  DutyTimelineEntity,
  OperationalEntity,
} from '@features/operational/domain/entities/operational.entity';
import type {
  OperationalItemDtoType,
  OperationalTimelineItemDtoType,
} from '@features/operational/presentation/dtos/operational-response.dto';

export class OperationalResponseMapper {
  public static toTimelineResponse(timeline: DutyTimelineEntity): OperationalTimelineItemDtoType {
    return {
      id: timeline.id,
      journey_uid: timeline.journeyUid,
      duty: timeline.duty,
      state: timeline.state,
      duty_dt: timeline.dutyDt,
      state_dt: timeline.stateDt,
    };
  }

  public static toResponse(operational: OperationalEntity): OperationalItemDtoType {
    return {
      id: operational.id,
      user_email: operational.userEmail,
      user_name: operational.userName,
      user_profile_pic: operational.userProfilePic,
      group_name: operational.groupName,
      latest_duty: operational.latestDuty,
      latest_state: operational.latestState,
      latest_duty_dt: operational.latestDutyDt,
      latest_state_dt: operational.latestStateDt,
      total_activities: operational.totalActivities,
      duty_date: operational.dutyDate,
      timeline: operational.timeline.map(OperationalResponseMapper.toTimelineResponse),
    };
  }

  public static toResponseList(operationals: OperationalEntity[]): OperationalItemDtoType[] {
    return operationals.map(OperationalResponseMapper.toResponse);
  }
}

import type { AttendanceEntity } from '@features/attendance/domain/entities/attendance.entity';
import type { AttendanceItemDtoType } from '@features/attendance/presentation/dtos/attendance-response.dto';

export class AttendanceResponseMapper {
  public static toResponse(attendance: AttendanceEntity): AttendanceItemDtoType {
    return {
      id: attendance.id,
      user_email: attendance.userEmail,
      user_name: attendance.userName,
      user_profile_pic: attendance.userProfilePic,
      user_duty: attendance.userDuty,
      attendance_dt: attendance.attendanceDt,
      attendance_type: attendance.attendanceType,
      category: attendance.category,
      latitude: attendance.latitude,
      longitude: attendance.longitude,
      diff_minutes: attendance.diffMinutes,
      location_name: attendance.locationName,
      shift_name: attendance.shiftName,
      shift_color: attendance.shiftColor,
      shift_time_window: attendance.shiftTimeWindow,
      group_name: attendance.groupName,
    };
  }
  public static toResponseList(attendances: AttendanceEntity[]): AttendanceItemDtoType[] {
    return attendances.map(AttendanceResponseMapper.toResponse);
  }
}

export type AttendanceCategoryType = 'masuk' | 'pulang' | 'lainnya';

export interface AttendanceProps {
  id: string;
  userEmail?: string | null;
  userName?: string | null;
  userProfilePic?: string | null;
  userDuty?: string | null;
  attendanceDt?: string | null;
  attendanceType?: string | null;
  latitude?: string | null;
  longitude?: string | null;
  diffMinutes: number;
  locationName?: string | null;
  shiftName?: string | null;
  shiftColor?: string | null;
  shiftTimeWindow?: string | null;
  groupName?: string | null;
}

export class AttendanceEntity {
  public readonly id: string;
  public readonly userEmail: string | null;
  public readonly userName: string | null;
  public readonly userProfilePic: string | null;
  public readonly userDuty: string | null;
  public readonly attendanceDt: string | null;
  public readonly attendanceType: string | null;
  public readonly latitude: string | null;
  public readonly longitude: string | null;
  public readonly diffMinutes: number;
  public readonly locationName: string | null;
  public readonly shiftName: string | null;
  public readonly shiftColor: string | null;
  public readonly shiftTimeWindow: string | null;
  public readonly groupName: string | null;

  constructor(props: AttendanceProps) {
    this.id = props.id;
    this.userEmail = props.userEmail ?? null;
    this.userName = props.userName ?? null;
    this.userProfilePic = props.userProfilePic ?? null;
    this.userDuty = props.userDuty ?? null;
    this.attendanceDt = props.attendanceDt ?? null;
    this.attendanceType = props.attendanceType ?? null;
    this.latitude = props.latitude ?? null;
    this.longitude = props.longitude ?? null;
    this.diffMinutes = props.diffMinutes;
    this.locationName = props.locationName ?? null;
    this.shiftName = props.shiftName ?? null;
    this.shiftColor = props.shiftColor ?? null;
    this.shiftTimeWindow = props.shiftTimeWindow ?? null;
    this.groupName = props.groupName ?? null;
  }

  public get category(): AttendanceCategoryType {
    const type = this.attendanceType?.toLowerCase();
    if (type?.includes('check in') || type?.includes('masuk')) {
      return 'masuk';
    }
    if (type?.includes('check out') || type?.includes('pulang')) {
      return 'pulang';
    }
    return 'lainnya';
  }
}

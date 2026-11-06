export interface DutyTimelineProps {
  id: string;
  journeyUid?: string | null;
  duty?: string | null;
  state?: string | null;
  dutyDt?: string | null;
  stateDt?: string | null;
}

export class DutyTimelineEntity {
  public readonly id: string;
  public readonly journeyUid: string | null;
  public readonly duty: string | null;
  public readonly state: string | null;
  public readonly dutyDt: string | null;
  public readonly stateDt: string | null;

  constructor(props: DutyTimelineProps) {
    this.id = props.id;
    this.journeyUid = props.journeyUid ?? null;
    this.duty = props.duty ?? null;
    this.state = props.state ?? null;
    this.dutyDt = props.dutyDt ?? null;
    this.stateDt = props.stateDt ?? null;
  }
}

export interface OperationalProps {
  id: string;
  userEmail: string;
  userName?: string | null;
  userProfilePic?: string | null;
  groupName?: string | null;
  latestDuty?: string | null;
  latestState?: string | null;
  latestDutyDt?: string | null;
  latestStateDt?: string | null;
  totalActivities: number;
  dutyDate?: string | null;
  timeline: DutyTimelineEntity[];
}

export class OperationalEntity {
  public readonly id: string;
  public readonly userEmail: string;
  public readonly userName: string | null;
  public readonly userProfilePic: string | null;
  public readonly groupName: string | null;
  public readonly latestDuty: string | null;
  public readonly latestState: string | null;
  public readonly latestDutyDt: string | null;
  public readonly latestStateDt: string | null;
  public readonly totalActivities: number;
  public readonly dutyDate: string | null;
  public readonly timeline: DutyTimelineEntity[];

  constructor(props: OperationalProps) {
    this.id = props.id;
    this.userEmail = props.userEmail;
    this.userName = props.userName ?? null;
    this.userProfilePic = props.userProfilePic ?? null;
    this.groupName = props.groupName ?? null;
    this.latestDuty = props.latestDuty ?? null;
    this.latestState = props.latestState ?? null;
    this.latestDutyDt = props.latestDutyDt ?? null;
    this.latestStateDt = props.latestStateDt ?? null;
    this.totalActivities = props.totalActivities;
    this.dutyDate = props.dutyDate ?? null;
    this.timeline = props.timeline;
  }
}

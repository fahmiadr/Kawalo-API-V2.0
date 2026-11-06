export interface InformationProps {
  id: string;
  title?: string | null;
  text?: string | null;
  userEmail?: string | null;
  userName?: string | null;
  userProfilePic?: string | null;
  feedDt?: string | null;
  feedFile?: string | null;
  photos?: string[];
  feedType?: string | null;
  groupName?: string | null;
  eventLocation?: string | null;
  eventStartDt?: string | null;
  eventEndDt?: string | null;
  eventDuration?: number | null;
  glat?: string | null;
  glong?: string | null;
}

export class InformationEntity {
  public readonly id: string;
  public readonly title: string | null;
  public readonly text: string | null;
  public readonly userEmail: string | null;
  public readonly userName: string | null;
  public readonly userProfilePic: string | null;
  public readonly feedDt: string | null;
  public readonly feedFile: string | null;
  public readonly photos: string[];
  public readonly feedType: string | null;
  public readonly groupName: string | null;
  public readonly eventLocation: string | null;
  public readonly eventStartDt: string | null;
  public readonly eventEndDt: string | null;
  public readonly eventDuration: number | null;
  public readonly glat: string | null;
  public readonly glong: string | null;

  constructor(props: InformationProps) {
    this.id = props.id;
    this.title = props.title ?? null;
    this.text = props.text ?? null;
    this.userEmail = props.userEmail ?? null;
    this.userName = props.userName ?? null;
    this.userProfilePic = props.userProfilePic ?? null;
    this.feedDt = props.feedDt ?? null;
    this.feedFile = props.feedFile ?? null;
    this.photos = props.photos ?? [];
    this.feedType = props.feedType ?? null;
    this.groupName = props.groupName ?? null;
    this.eventLocation = props.eventLocation ?? null;
    this.eventStartDt = props.eventStartDt ?? null;
    this.eventEndDt = props.eventEndDt ?? null;
    this.eventDuration = props.eventDuration ?? null;
    this.glat = props.glat ?? null;
    this.glong = props.glong ?? null;
  }
}

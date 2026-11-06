export interface GuestProps {
  id: string;
  guestUid?: string | null;
  groupUid?: string | null;
  groupName?: string | null;
  guestName?: string | null;
  guestNumber?: string | null;
  guestIdentity?: string | null;
  guestIdentityType?: string | null;
  guestPhoto?: string | null;
  guestDt?: string | null;
  guestAckDt?: string | null;
  guestOutDt?: string | null;
  guestPurpose?: string | null;
  guestDestination?: string | null;
  guestToUserEmail?: string | null;
  guestToUserName?: string | null;
  guestCreatorEmail?: string | null;
  guestCreatorName?: string | null;
  guestCategory?: string | null;
  qrUid?: string | null;
  guestAddress?: string | null;
  guestType?: string | null;
  guestCheckOutByEmail?: string | null;
  guestCheckOutByName?: string | null;
}

export class GuestEntity {
  public readonly id: string;
  public readonly guestUid: string | null;
  public readonly groupUid: string | null;
  public readonly groupName: string | null;
  public readonly guestName: string | null;
  public readonly guestNumber: string | null;
  public readonly guestIdentity: string | null;
  public readonly guestIdentityType: string | null;
  public readonly guestPhoto: string | null;
  public readonly guestDt: string | null;
  public readonly guestAckDt: string | null;
  public readonly guestOutDt: string | null;
  public readonly guestPurpose: string | null;
  public readonly guestDestination: string | null;
  public readonly guestToUserEmail: string | null;
  public readonly guestToUserName: string | null;
  public readonly guestCreatorEmail: string | null;
  public readonly guestCreatorName: string | null;
  public readonly guestCategory: string | null;
  public readonly qrUid: string | null;
  public readonly guestAddress: string | null;
  public readonly guestType: string | null;
  public readonly guestCheckOutByEmail: string | null;
  public readonly guestCheckOutByName: string | null;

  constructor(props: GuestProps) {
    this.id = props.id;
    this.guestUid = props.guestUid ?? null;
    this.groupUid = props.groupUid ?? null;
    this.groupName = props.groupName ?? null;
    this.guestName = props.guestName ?? null;
    this.guestNumber = props.guestNumber ?? null;
    this.guestIdentity = props.guestIdentity ?? null;
    this.guestIdentityType = props.guestIdentityType ?? null;
    this.guestPhoto = props.guestPhoto ?? null;
    this.guestDt = props.guestDt ?? null;
    this.guestAckDt = props.guestAckDt ?? null;
    this.guestOutDt = props.guestOutDt ?? null;
    this.guestPurpose = props.guestPurpose ?? null;
    this.guestDestination = props.guestDestination ?? null;
    this.guestToUserEmail = props.guestToUserEmail ?? null;
    this.guestToUserName = props.guestToUserName ?? null;
    this.guestCreatorEmail = props.guestCreatorEmail ?? null;
    this.guestCreatorName = props.guestCreatorName ?? null;
    this.guestCategory = props.guestCategory ?? null;
    this.qrUid = props.qrUid ?? null;
    this.guestAddress = props.guestAddress ?? null;
    this.guestType = props.guestType ?? null;
    this.guestCheckOutByEmail = props.guestCheckOutByEmail ?? null;
    this.guestCheckOutByName = props.guestCheckOutByName ?? null;
  }
}

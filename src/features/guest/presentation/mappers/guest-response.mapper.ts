import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';
import type {
  DeleteBatchGuestResponseDtoType,
  GuestItemDtoType,
  MasterGuestTypeItemDtoType,
} from '@features/guest/presentation/dtos/guest-response.dto';
import type { DeleteGuestOutput } from '@features/guest/usecases/delete-guest.usecase';

export class GuestResponseMapper {
  public static toResponse(guest: GuestEntity): GuestItemDtoType {
    return {
      id: guest.id,
      group_name: guest.groupName,
      guest_name: guest.guestName,
      guest_number: guest.guestNumber,
      guest_identity: guest.guestIdentity,
      guest_identity_type: guest.guestIdentityType,
      guest_photo: guest.guestPhoto,
      guest_dt: guest.guestDt,
      guest_ack_dt: guest.guestAckDt,
      guest_out_dt: guest.guestOutDt,
      guest_purpose: guest.guestPurpose,
      guest_destination: guest.guestDestination,
      guest_to_user_email: guest.guestToUserEmail,
      guest_to_user_name: guest.guestToUserName,
      guest_creator_email: guest.guestCreatorEmail,
      guest_creator_name: guest.guestCreatorName,
      guest_category: guest.guestCategory,
      qr_uid: guest.qrUid,
      guest_address: guest.guestAddress,
      guest_type: guest.guestType,
      guest_checkout_by_email: guest.guestCheckOutByEmail,
      guest_checkout_by_name: guest.guestCheckOutByName,
    };
  }

  public static toResponseList(guests: GuestEntity[]): GuestItemDtoType[] {
    return guests.map(GuestResponseMapper.toResponse);
  }

  public static toMasterGuestType(item: MasterGuestTypeItem): MasterGuestTypeItemDtoType {
    return {
      id: item.id,
      uid: item.uid,
      name: item.name,
    };
  }

  public static toMasterGuestTypeList(items: MasterGuestTypeItem[]): MasterGuestTypeItemDtoType[] {
    return items.map(GuestResponseMapper.toMasterGuestType);
  }

  public static toDeleteBatchResponse(output: DeleteGuestOutput): DeleteBatchGuestResponseDtoType {
    return {
      deleted_count: output.deletedCount,
    };
  }
}

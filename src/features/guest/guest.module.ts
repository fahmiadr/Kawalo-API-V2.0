import { GuestRepositoryImpl } from '@features/guest/data/repositories/guest.repository.impl';
import { GuestHandler } from '@features/guest/presentation/handlers/guest.handler';
import { AcknowledgeGuestUseCase } from '@features/guest/usecases/acknowledge-guest.usecase';
import { CheckoutGuestUseCase } from '@features/guest/usecases/checkout-guest.usecase';
import { CreateGuestUseCase } from '@features/guest/usecases/create-guest.usecase';
import { DeleteGuestUseCase } from '@features/guest/usecases/delete-guest.usecase';
import { GetGuestByIdUseCase } from '@features/guest/usecases/get-guest-by-id.usecase';
import { GetGuestsUseCase } from '@features/guest/usecases/get-guests.usecase';
import { GetMasterGuestTypesUseCase } from '@features/guest/usecases/get-master-guest-types.usecase';
import { UpdateGuestUseCase } from '@features/guest/usecases/update-guest.usecase';

const guestRepository = new GuestRepositoryImpl();
const getGuestsUseCase = new GetGuestsUseCase(guestRepository);
const getGuestByIdUseCase = new GetGuestByIdUseCase(guestRepository);
const createGuestUseCase = new CreateGuestUseCase(guestRepository);
const updateGuestUseCase = new UpdateGuestUseCase(guestRepository);
const deleteGuestUseCase = new DeleteGuestUseCase(guestRepository);
const acknowledgeGuestUseCase = new AcknowledgeGuestUseCase(guestRepository);
const checkoutGuestUseCase = new CheckoutGuestUseCase(guestRepository);
const getMasterGuestTypesUseCase = new GetMasterGuestTypesUseCase(guestRepository);

export const guestHandler = new GuestHandler(
  getGuestsUseCase,
  getGuestByIdUseCase,
  createGuestUseCase,
  updateGuestUseCase,
  deleteGuestUseCase,
  acknowledgeGuestUseCase,
  checkoutGuestUseCase,
  getMasterGuestTypesUseCase,
);

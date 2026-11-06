import { InformationRepositoryImpl } from '@features/information/data/repositories/information.repository.impl';
import { InformationHandler } from '@features/information/presentation/handlers/information.handler';
import { CreateInformationUseCase } from '@features/information/usecases/create-information.usecase';
import { DeleteInformationUseCase } from '@features/information/usecases/delete-information.usecase';
import { GetInformationUseCase } from '@features/information/usecases/get-information.usecase';
import { UpdateInformationUseCase } from '@features/information/usecases/update-information.usecase';

const informationRepository = new InformationRepositoryImpl();
const getInformationUseCase = new GetInformationUseCase(informationRepository);
const createInformationUseCase = new CreateInformationUseCase(informationRepository);
const updateInformationUseCase = new UpdateInformationUseCase(informationRepository);
const deleteInformationUseCase = new DeleteInformationUseCase(informationRepository);

export const informationHandler = new InformationHandler(
  getInformationUseCase,
  createInformationUseCase,
  updateInformationUseCase,
  deleteInformationUseCase,
);

import { OperationalRepositoryImpl } from '@features/operational/data/repositories/operational.repository.impl';
import { OperationalHandler } from '@features/operational/presentation/handlers/operational.handler';
import { GetOperationalDataUseCase } from '@features/operational/usecases/get-operational-data.usecase';

const operationalRepository = new OperationalRepositoryImpl();
const getOperationalDataUseCase = new GetOperationalDataUseCase(operationalRepository);

export const operationalHandler = new OperationalHandler(getOperationalDataUseCase);

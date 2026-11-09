import { IncidentRepositoryImpl } from '@features/incident/data/repositories/incident.repository.impl';
import { IncidentEventBus } from '@features/incident/infrastructure/incident-event-bus';
import { IncidentHandler } from '@features/incident/presentation/handlers/incident.handler';
import { CreateIncidentUseCase } from '@features/incident/usecases/create-incident.usecase';
import { DeleteIncidentUseCase } from '@features/incident/usecases/delete-incident.usecase';
import { GetIncidentDetailUseCase } from '@features/incident/usecases/get-incident-detail.usecase';
import { GetIncidentsUseCase } from '@features/incident/usecases/get-incidents.usecase';
import { PostIncidentReplyUseCase } from '@features/incident/usecases/post-incident-reply.usecase';
import { ResolveIncidentUseCase } from '@features/incident/usecases/resolve-incident.usecase';

const repository = new IncidentRepositoryImpl();
const eventBus = new IncidentEventBus();

export const incidentHandler = new IncidentHandler(
  new GetIncidentsUseCase(repository),
  new GetIncidentDetailUseCase(repository),
  new CreateIncidentUseCase(repository),
  new PostIncidentReplyUseCase(repository, eventBus),
  new ResolveIncidentUseCase(repository, eventBus),
  new DeleteIncidentUseCase(repository, eventBus),
  eventBus,
);

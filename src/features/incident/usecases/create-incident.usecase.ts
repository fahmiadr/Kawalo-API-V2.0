import { type Failure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import { generatePrefixedUuidV7 } from '@core/utils/uuid.util';
import {
  INCIDENT_CATEGORIES,
  type IncidentEntity,
} from '@features/incident/domain/entities/incident.entity';
import type { IncidentRepository } from '@features/incident/domain/repositories/incident.repository';

export interface CreateIncidentInput {
  currentUser: { email: string; name?: string; group_uid?: string | null };
  data: {
    title: string;
    description?: string;
    category?: string;
    glat?: number;
    glong?: number;
    photos: string[];
  };
}

export class CreateIncidentUseCase implements UseCase<IncidentEntity, CreateIncidentInput> {
  constructor(private readonly repository: IncidentRepository) {}

  public async execute({
    currentUser,
    data,
  }: CreateIncidentInput): Promise<Result<IncidentEntity, Failure>> {
    const groupUid = currentUser.group_uid;
    if (!groupUid) return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    if (!data.title) return Result.fail(new ValidationFailure('Judul insiden wajib diisi.'));

    const category = (data.category ?? 'general').toLowerCase();
    if (!(INCIDENT_CATEGORIES as readonly string[]).includes(category)) {
      return Result.fail(
        new ValidationFailure(`Kategori harus salah satu dari: ${INCIDENT_CATEGORIES.join(', ')}.`),
      );
    }
    if ((data.glat === undefined) !== (data.glong === undefined)) {
      return Result.fail(new ValidationFailure('Koordinat harus diisi lengkap (glat dan glong).'));
    }

    const created = await this.repository.create({
      id: generatePrefixedUuidV7('inc'),
      groupUid,
      title: data.title,
      description: data.description ?? null,
      category,
      glat: data.glat ?? null,
      glong: data.glong ?? null,
      photos: data.photos,
      createdBy: currentUser.email,
      createdByName: currentUser.name ?? currentUser.email,
    });
    return Result.ok(created);
  }
}

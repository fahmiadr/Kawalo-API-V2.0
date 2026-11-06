import { AttendanceRepositoryImpl } from '@features/attendance/data/repositories/attendance.repository.impl';
import { AttendanceHandler } from '@features/attendance/presentation/handlers/attendance.handler';
import { GetAttendancesUseCase } from '@features/attendance/usecases/get-attendances.usecase';

/**
 * Feature Composition Root untuk modul Absensi.
 *
 * Bertindak sebagai satu-satunya tempat perakitan Dependency Injection
 * di tingkat fitur, sehingga layer Data, Domain, dan Presentation tetap
 * terisolasi murni satu sama lain tanpa kebocoran impor lintas layer.
 */
const attendanceRepository = new AttendanceRepositoryImpl();
const getAttendancesUseCase = new GetAttendancesUseCase(attendanceRepository);

export const attendanceHandler = new AttendanceHandler(getAttendancesUseCase);

-- Manual attendance requests: the escape hatch when the face check fails three times or the
-- camera is unavailable. An admin of the group approves (which writes a `tkabsen` row marked
-- unverified: AFaceVerified = 0, no photo) or rejects. Plain CREATE only: runs on MariaDB and MySQL 9.
--
-- Reason: face_failed | camera_unavailable. AbsenType: CHECK IN | CHECK OUT as decided at request
-- time. Status: PENDING | APPROVED | REJECTED. PendingKey is `email|YYYY-MM-DD` while PENDING and
-- NULL once decided; its unique index allows one open request per user per day (NULLs never clash).
-- AbsenID points at the row created on approval.
CREATE TABLE `tkattendancemanual` (
    `ManualId` INTEGER NOT NULL AUTO_INCREMENT,
    `UserEmail` VARCHAR(128) NOT NULL,
    `UserName` VARCHAR(128) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `Reason` VARCHAR(24) NOT NULL,
    `Note` VARCHAR(200) NULL,
    `AbsenType` VARCHAR(16) NOT NULL,
    `Latitude` DOUBLE NOT NULL,
    `Longitude` DOUBLE NOT NULL,
    `DistanceM` INTEGER NULL,
    `RadiusM` INTEGER NULL,
    `ScheduleId` INTEGER NOT NULL,
    `ShiftUID` VARCHAR(64) NOT NULL,
    `ShiftName` VARCHAR(128) NOT NULL,
    `ShiftWindow` VARCHAR(16) NOT NULL,
    `LocName` VARCHAR(64) NOT NULL,
    `FailedAttempts` INTEGER NOT NULL DEFAULT 0,
    `Status` VARCHAR(12) NOT NULL DEFAULT 'PENDING',
    `PendingKey` VARCHAR(190) NULL,
    `RequestedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `DecidedBy` VARCHAR(128) NULL,
    `DecidedAt` TIMESTAMP(0) NULL DEFAULT NULL,
    `DecisionNote` VARCHAR(200) NULL,
    `AbsenID` INTEGER NULL,

    UNIQUE INDEX `uq_tkattendancemanual_pending`(`PendingKey`),
    INDEX `idx_tkattendancemanual_group`(`GroupUID`, `Status`, `RequestedAt`),
    INDEX `idx_tkattendancemanual_absen`(`AbsenID`),
    PRIMARY KEY (`ManualId`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

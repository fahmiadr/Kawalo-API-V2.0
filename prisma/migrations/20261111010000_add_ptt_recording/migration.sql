-- CreateTable
CREATE TABLE `tkpttrecording` (
    `RecordingUID` VARCHAR(64) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `SpeakerUserId` VARCHAR(128) NOT NULL,
    `SpeakerEmail` VARCHAR(128) NULL,
    `SpeakerName` VARCHAR(128) NOT NULL,
    `SpeakerRole` VARCHAR(16) NOT NULL,
    `StartedAt` TIMESTAMP(3) NOT NULL,
    `DurationMs` INTEGER NOT NULL,
    `SizeBytes` INTEGER NOT NULL,
    `FilePath` VARCHAR(255) NOT NULL,
    `CreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `DeletedAt` TIMESTAMP(0) NULL DEFAULT NULL,

    INDEX `idx_tkpttrecording_group_started`(`GroupUID`, `StartedAt`, `RecordingUID`),
    PRIMARY KEY (`RecordingUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- AlterTable: transcript of a recorded PTT turn. Filled by the speaker's device (Android 13+ speech
-- recogniser) or, when that is missing, by the server-side worker. Plain ALTER: runs on MariaDB and MySQL.
-- TranscriptStatus: NULL (no news yet), done, unsupported, failed, processing, no_speech, error.
ALTER TABLE `tkpttrecording`
    ADD COLUMN `RequestId` VARCHAR(64) NULL,
    ADD COLUMN `Transcript` TEXT NULL,
    ADD COLUMN `TranscriptLocale` VARCHAR(16) NULL,
    ADD COLUMN `TranscriptAt` TIMESTAMP(0) NULL DEFAULT NULL,
    ADD COLUMN `TranscriptSource` VARCHAR(8) NULL,
    ADD COLUMN `TranscriptStatus` VARCHAR(16) NULL,
    ADD COLUMN `TranscriptAttempts` TINYINT UNSIGNED NOT NULL DEFAULT 0,
    ADD COLUMN `TranscriptClaimedAt` TIMESTAMP(0) NULL DEFAULT NULL,
    ADD INDEX `idx_tkpttrecording_request`(`GroupUID`, `SpeakerUserId`, `RequestId`),
    ADD INDEX `idx_tkpttrecording_transcript_queue`(`TranscriptStatus`, `StartedAt`);

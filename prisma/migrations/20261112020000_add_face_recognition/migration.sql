-- Face recognition for check-in/out. Plain CREATE/ALTER only: runs on MariaDB and MySQL 9.
-- Biometric data is encrypted by the API (AES-256-GCM, FACE_ENCRYPTION_KEY) before it reaches
-- these tables; photo files live in the private FACE_PHOTO_DIR, never in UPLOAD_DIR.

-- One embedding per user (the face does not change per group). `Embedding` is
-- nonce || ciphertext || tag of 512 little-endian f32, bound to `UserEmail`.
-- `ModelVersion` names the embedder weights; another version forces re-registration.
CREATE TABLE `tkfaceembedding` (
    `UserEmail` VARCHAR(128) NOT NULL,
    `Embedding` BLOB NOT NULL,
    `ModelVersion` VARCHAR(64) NOT NULL,
    `PhotoCount` INTEGER NOT NULL,
    `CreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    PRIMARY KEY (`UserEmail`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Registration photos that passed the quality gate; `FilePath` is relative to FACE_PHOTO_DIR
-- (random file name, encrypted content). Deleted together with the embedding.
CREATE TABLE `tkfacephoto` (
    `PhotoUID` VARCHAR(64) NOT NULL,
    `UserEmail` VARCHAR(128) NOT NULL,
    `FilePath` VARCHAR(255) NOT NULL,
    `CreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    INDEX `idx_tkfacephoto_email`(`UserEmail`),
    PRIMARY KEY (`PhotoUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Who registered, deleted or viewed biometric data. Action: REGISTER, DUPLICATE_BLOCKED,
-- DELETE_SELF, DELETE_BY_ADMIN, VIEW_REGISTER_PHOTO, VIEW_ATTENDANCE_PHOTO.
CREATE TABLE `tkfaceaudit` (
    `AuditId` INTEGER NOT NULL AUTO_INCREMENT,
    `Action` VARCHAR(32) NOT NULL,
    `ActorEmail` VARCHAR(128) NOT NULL,
    `TargetEmail` VARCHAR(128) NOT NULL,
    `GroupUID` VARCHAR(64) NULL,
    `Reference` VARCHAR(128) NULL,
    `CreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    INDEX `idx_tkfaceaudit_target`(`TargetEmail`, `CreatedAt`),
    PRIMARY KEY (`AuditId`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- AEarlyMins: minutes a check-out came before the shift end (0 within the tolerance), the
-- counterpart of ADiffMins for check-ins. AFaceVerified/AFaceScore: 1:1 match result (legacy and
-- any future manual rows stay 0/NULL). APhotoPath: encrypted check-in/out photo, relative to
-- FACE_PHOTO_DIR.
ALTER TABLE `tkabsen`
    ADD COLUMN `AEarlyMins` INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN `AFaceVerified` TINYINT(1) NOT NULL DEFAULT 0,
    ADD COLUMN `AFaceScore` FLOAT NULL,
    ADD COLUMN `APhotoPath` VARCHAR(255) NULL;

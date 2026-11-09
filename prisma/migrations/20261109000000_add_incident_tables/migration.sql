-- CreateTable
CREATE TABLE `tkincident` (
    `IncidentUID` VARCHAR(64) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `Title` VARCHAR(128) NOT NULL,
    `Description` VARCHAR(1024) NULL,
    `Category` VARCHAR(16) NOT NULL DEFAULT 'general',
    `GLat` DOUBLE NULL,
    `GLong` DOUBLE NULL,
    `Photos` TEXT NULL,
    `Status` VARCHAR(16) NOT NULL DEFAULT 'OPEN',
    `CreatedByEmail` VARCHAR(128) NOT NULL,
    `CreatedByName` VARCHAR(128) NULL,
    `ResolvedByEmail` VARCHAR(128) NULL,
    `ResolvedByName` VARCHAR(128) NULL,
    `ResolvedAt` TIMESTAMP(0) NULL DEFAULT NULL,
    `DeletedAt` TIMESTAMP(0) NULL DEFAULT NULL,
    `CreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `UpdatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    INDEX `idx_tkincident_group_status_dt`(`GroupUID`, `Status`, `CreatedAt`),
    PRIMARY KEY (`IncidentUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkincidentreply` (
    `ReplyUID` VARCHAR(64) NOT NULL,
    `IncidentUID` VARCHAR(64) NOT NULL,
    `SenderEmail` VARCHAR(128) NOT NULL,
    `SenderName` VARCHAR(128) NOT NULL,
    `SenderRole` VARCHAR(16) NOT NULL,
    `Message` TEXT NOT NULL,
    `Photos` TEXT NULL,
    `CreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    INDEX `idx_tkincidentreply_incident`(`IncidentUID`, `ReplyUID`),
    PRIMARY KEY (`ReplyUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

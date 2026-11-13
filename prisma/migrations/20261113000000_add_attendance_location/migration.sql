-- Per-group attendance point: one location and radius that every shift of the group checks in at.
-- A group without a row falls back to the legacy tkshift.Loc* columns.
-- Plain CREATE only: runs on MariaDB and MySQL 9.
CREATE TABLE `tkattendancelocation` (
    `GroupUID` VARCHAR(64) NOT NULL,
    `LocName` VARCHAR(128) NOT NULL,
    `LocLat` DOUBLE NOT NULL,
    `LocLong` DOUBLE NOT NULL,
    `LocRadius` INTEGER NOT NULL,
    `UpdatedBy` VARCHAR(128) NOT NULL,
    `UpdatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    PRIMARY KEY (`GroupUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

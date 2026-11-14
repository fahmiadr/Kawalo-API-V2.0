-- Home screen card layout per user and active group. `Layout` is JSON text
-- {"order": [...], "hidden": [...]}; no row means the default layout.
-- Plain CREATE only: runs on MariaDB and MySQL 9.
CREATE TABLE `tkhomelayout` (
    `UserEmail` VARCHAR(128) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `Layout` TEXT NOT NULL,
    `UpdatedAt` DATETIME(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    PRIMARY KEY (`UserEmail`, `GroupUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Per-group attendance settings. FaceMatchPercent is the minimum face similarity (whole percent)
-- a check-in/out must reach; a group without a row uses the FACE_MATCH_THRESHOLD default.
-- Plain CREATE only: runs on MariaDB and MySQL 9.
CREATE TABLE `tkattendancesetting` (
    `GroupUID` VARCHAR(64) NOT NULL,
    `FaceMatchPercent` INTEGER NOT NULL,
    `UpdatedBy` VARCHAR(128) NOT NULL,
    `UpdatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    PRIMARY KEY (`GroupUID`)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

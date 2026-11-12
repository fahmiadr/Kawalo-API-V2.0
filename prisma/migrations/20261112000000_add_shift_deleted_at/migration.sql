-- Soft delete for shifts; plain ALTER keeps this portable between MariaDB and MySQL.
ALTER TABLE `tkshift` ADD COLUMN `ShiftDeletedAt` DATETIME NULL;

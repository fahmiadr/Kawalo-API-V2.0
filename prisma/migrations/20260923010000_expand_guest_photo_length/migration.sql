-- Clean legacy zero-datetime values in tkguest to NULL so ALTER TABLE does not fail in MySQL strict mode
UPDATE `tkguest` SET `GuestAckDT` = NULL WHERE CAST(`GuestAckDT` AS CHAR) = '0000-00-00 00:00:00';
UPDATE `tkguest` SET `GuestOutDT` = NULL WHERE CAST(`GuestOutDT` AS CHAR) = '0000-00-00 00:00:00';

-- AlterTable: Expand GuestPhoto length in tkguest to standard URL limit (2048)
ALTER TABLE `tkguest`
  MODIFY `GuestPhoto` VARCHAR(2048) NULL;

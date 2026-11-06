-- AlterTable: Remove erroneous defaults from GuestOutDT and GuestAckDT
ALTER TABLE `tkguest` 
  MODIFY `GuestOutDT` TIMESTAMP NULL DEFAULT NULL,
  MODIFY `GuestAckDT` TIMESTAMP NULL DEFAULT NULL;

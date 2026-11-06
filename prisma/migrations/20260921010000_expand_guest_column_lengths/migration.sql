-- AlterTable: Expand column lengths in tkguest to prevent truncation and align with tkuser
ALTER TABLE `tkguest`
  MODIFY `GuestToUserEmail` VARCHAR(128) NULL,
  MODIFY `GuestCreatorEmail` VARCHAR(128) NULL,
  MODIFY `GuestCheckOutByEmail` VARCHAR(128) NULL,
  MODIFY `GuestCreatorName` VARCHAR(128) NULL,
  MODIFY `GuestNumber` VARCHAR(32) NULL,
  MODIFY `GuestIdentityType` VARCHAR(32) NULL;

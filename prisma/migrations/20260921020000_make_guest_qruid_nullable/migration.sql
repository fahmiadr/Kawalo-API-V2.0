-- AlterTable: Make QRUID nullable in tkguest to eliminate fake data generation and prevent data poisoning
ALTER TABLE `tkguest`
    MODIFY `QRUID` VARCHAR(64) NULL DEFAULT NULL;

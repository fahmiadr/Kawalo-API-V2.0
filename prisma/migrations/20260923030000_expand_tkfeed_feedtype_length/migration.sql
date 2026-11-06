-- AlterTable: Expand FeedType length in tkfeed to match GuestType (64)
ALTER TABLE `tkfeed`
  MODIFY `FeedType` VARCHAR(64) NULL;

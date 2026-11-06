-- AlterTable: Make coordinates and additional photos nullable with DEFAULT NULL in tkfeed
ALTER TABLE `tkfeed`
  MODIFY `GLat` VARCHAR(32) NULL DEFAULT NULL,
  MODIFY `GLong` VARCHAR(32) NULL DEFAULT NULL,
  MODIFY `FeedPic2` VARCHAR(2048) NULL DEFAULT NULL,
  MODIFY `FeedPic3` VARCHAR(2048) NULL DEFAULT NULL,
  MODIFY `FeedPic4` VARCHAR(2048) NULL DEFAULT NULL;

-- Convert existing dummy 'N/A' strings to true NULL (preserving all real data)
UPDATE `tkfeed` SET `GLat` = NULL WHERE `GLat` = 'N/A';
UPDATE `tkfeed` SET `GLong` = NULL WHERE `GLong` = 'N/A';
UPDATE `tkfeed` SET `FeedPic2` = NULL WHERE `FeedPic2` = 'N/A';
UPDATE `tkfeed` SET `FeedPic3` = NULL WHERE `FeedPic3` = 'N/A';
UPDATE `tkfeed` SET `FeedPic4` = NULL WHERE `FeedPic4` = 'N/A';

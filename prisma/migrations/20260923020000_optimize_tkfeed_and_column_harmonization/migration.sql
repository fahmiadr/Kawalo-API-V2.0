-- Step 1: Clean legacy zero-datetime values in nullable columns to prevent MySQL error 1292
UPDATE `tkfeed` SET `FeedEventStartDT` = NULL WHERE CAST(`FeedEventStartDT` AS CHAR) = '0000-00-00 00:00:00';
UPDATE `tkfeed` SET `FeedEventEndDT` = NULL WHERE CAST(`FeedEventEndDT` AS CHAR) = '0000-00-00 00:00:00';


-- Step 2: Alter tkfeed (Harmonize email, name, URLs, location and add composite index)
ALTER TABLE `tkfeed`
  MODIFY `FeedByEmail` VARCHAR(128) NOT NULL,
  MODIFY `FeedByName` VARCHAR(128) NULL,
  MODIFY `FeedFile` VARCHAR(2048) NULL,
  MODIFY `FeedPic` VARCHAR(2048) NULL,
  MODIFY `FeedPic2` VARCHAR(2048) NOT NULL DEFAULT 'N/A',
  MODIFY `FeedPic3` VARCHAR(2048) NOT NULL DEFAULT 'N/A',
  MODIFY `FeedPic4` VARCHAR(2048) NOT NULL DEFAULT 'N/A',
  MODIFY `FeedByProfilePic` VARCHAR(2048) NULL,
  MODIFY `FeedEventLocation` VARCHAR(128) NULL;

CREATE INDEX `idx_tkfeed_group_email` ON `tkfeed`(`FeedGroupUID`, `FeedByEmail`);

-- Step 3: Harmonize photo/file URLs across tables (standard URL limit: 2048)
ALTER TABLE `tkuser`
  MODIFY `UserProfilePic` VARCHAR(2048) NULL;

ALTER TABLE `tkgroup`
  MODIFY `GroupLogo` VARCHAR(2048) NULL;

ALTER TABLE `tkpatroljourney`
  MODIFY `Cam1` VARCHAR(2048) NULL,
  MODIFY `Cam2` VARCHAR(2048) NULL;

-- Step 4: Harmonize User Email & Name across tables (standard email/name limit: 128)
ALTER TABLE `tkguest`
  MODIFY `GuestName` VARCHAR(128) NULL;

ALTER TABLE `tkabsen`
  MODIFY `AbsenEmail` VARCHAR(128) NOT NULL DEFAULT 'N/A',
  MODIFY `AbsenGroupUID` VARCHAR(64) NULL;

ALTER TABLE `tkdutyjourney`
  MODIFY `DutyStateDT` TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  MODIFY `UserEmail` VARCHAR(128) NOT NULL,
  MODIFY `UserName` VARCHAR(128) NOT NULL,
  MODIFY `DutyGroupUID` VARCHAR(64) NOT NULL;

ALTER TABLE `tkusergroup`
  MODIFY `Email` VARCHAR(128) NOT NULL;

ALTER TABLE `tkschedule`
  MODIFY `SFriendEmail` VARCHAR(128) NOT NULL,
  MODIFY `SFriendName` VARCHAR(128) NOT NULL,
  MODIFY `SGroupUID` VARCHAR(64) NOT NULL;

ALTER TABLE `tkpatrolevent`
  MODIFY `PatEventGuardEmail` VARCHAR(128) NOT NULL,
  MODIFY `PatEventGuardName` VARCHAR(128) NOT NULL;

ALTER TABLE `tkpatroljourney`
  MODIFY `PatJourGuardEmail` VARCHAR(128) NOT NULL;

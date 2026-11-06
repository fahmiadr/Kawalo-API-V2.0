Γùç injected env (9) from .env.dev // tip: Γùê secrets for agents [www.dotenvx.com]
Γùç injected env (0) from .env // tip: Γîÿ multiple files { path: ['.env.local', '.env'] }
-- CreateTable
CREATE TABLE `chatconversations` (
    `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    `ms_user_email` VARCHAR(255) NULL,
    `target_ms_email` VARCHAR(255) NULL,
    `tk_user_id` INTEGER NULL,
    `firebase_ref_id` VARCHAR(255) NULL,
    `last_message_preview` TEXT NULL,
    `created_at` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `updated_at` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    UNIQUE INDEX `chatconversations_firebase_ref_id_unique`(`firebase_ref_id` ASC),
    INDEX `chatconversations_ms_user_email_index`(`ms_user_email` ASC),
    INDEX `chatconversations_target_ms_email_index`(`target_ms_email` ASC),
    INDEX `chatconversations_tk_user_id_index`(`tk_user_id` ASC),
    UNIQUE INDEX `uk_chat_ms_ms`(`ms_user_email` ASC, `target_ms_email` ASC),
    UNIQUE INDEX `uk_chat_ms_tk`(`ms_user_email` ASC, `tk_user_id` ASC),
    PRIMARY KEY (`id` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `chatmessages` (
    `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    `conversation_id` BIGINT UNSIGNED NULL,
    `sender_type` ENUM('MS', 'TK') NOT NULL,
    `sender_ref_id` VARCHAR(255) NOT NULL,
    `content` TEXT NOT NULL,
    `is_read` BOOLEAN NULL DEFAULT false,
    `created_at` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `updated_at` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    INDEX `chatmessages_conversation_id_foreign`(`conversation_id` ASC),
    PRIMARY KEY (`id` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `knex_migrations` (
    `id` INTEGER UNSIGNED NOT NULL AUTO_INCREMENT,
    `name` VARCHAR(255) NULL,
    `batch` INTEGER NULL,
    `migration_time` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    PRIMARY KEY (`id` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `knex_migrations_lock` (
    `index` INTEGER UNSIGNED NOT NULL AUTO_INCREMENT,
    `is_locked` INTEGER NULL,

    PRIMARY KEY (`index` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `msgroup` (
    `GroupID` INTEGER NOT NULL,
    `GroupName` VARCHAR(1024) NOT NULL,
    `PriveId` VARCHAR(64) NOT NULL,

    PRIMARY KEY (`GroupID` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `msuser` (
    `Email` VARCHAR(255) NOT NULL,
    `Password` VARCHAR(1024) NOT NULL,
    `GroupId` INTEGER NOT NULL,
    `Roles` ENUM('superadmin', 'admin', 'user') NOT NULL DEFAULT 'user',
    `ResetToken` VARCHAR(255) NULL,
    `ResetTokenExpiry` BIGINT NULL,

    INDEX `idx_msuser_groupid`(`GroupId` ASC),
    PRIMARY KEY (`Email` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tbl_marks` (
    `student_id` INTEGER UNSIGNED NOT NULL AUTO_INCREMENT,
    `student_name` VARCHAR(35) NOT NULL,
    `marks` INTEGER NULL DEFAULT 0,

    PRIMARY KEY (`student_id` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkabsen` (
    `AbsenID` INTEGER NOT NULL AUTO_INCREMENT,
    `AbsenEmail` VARCHAR(64) NOT NULL DEFAULT 'N/A',
    `AbsenName` VARCHAR(128) NOT NULL DEFAULT 'N/A',
    `AbsenDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `AbsenType` VARCHAR(128) NOT NULL DEFAULT 'REGISTER',
    `ALat` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `ALong` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `ADiffMins` INTEGER NOT NULL DEFAULT 0,
    `ALocation` VARCHAR(64) NOT NULL DEFAULT 'N/A',
    `AShiftUID` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `AbsenFunc` INTEGER NOT NULL DEFAULT 0,
    `AbsenGroupUID` VARCHAR(32) NULL,
    `AShStart` VARCHAR(9) NOT NULL DEFAULT '00:00:00',

    INDEX `idx_tkabsen_absendt`(`AbsenDT` ASC),
    INDEX `idx_tkabsen_email_dt`(`AbsenEmail` ASC, `AbsenDT` ASC),
    INDEX `idx_tkabsen_groupuid`(`AbsenGroupUID` ASC),
    PRIMARY KEY (`AbsenID` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkcheckpoint` (
    `CheckPointId` INTEGER NOT NULL AUTO_INCREMENT,
    `CheckPointUID` VARCHAR(64) NOT NULL,
    `CPGroupUID` VARCHAR(64) NOT NULL,
    `CPName` VARCHAR(128) NOT NULL,
    `CPType` VARCHAR(16) NOT NULL,
    `CPGPSLat` VARCHAR(32) NOT NULL,
    `CPGPSLong` VARCHAR(32) NOT NULL,
    `CPWifiSSID` VARCHAR(32) NOT NULL,
    `CPNFCIdentity` VARCHAR(64) NOT NULL,
    `CPQRIdentity` VARCHAR(128) NOT NULL,
    `CPWifiMAC` VARCHAR(64) NOT NULL,
    `CPDesc` VARCHAR(256) NOT NULL,
    `UserScope` VARCHAR(15) NOT NULL DEFAULT 'GUARD',
    `GPSRadius` INTEGER NOT NULL DEFAULT 10,

    UNIQUE INDEX `CheckPointUID`(`CheckPointUID` ASC),
    PRIMARY KEY (`CheckPointId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkdutyjourney` (
    `DutyJourneyId` INTEGER NOT NULL AUTO_INCREMENT,
    `DutyJourneyUID` VARCHAR(24) NOT NULL,
    `DutyGroupUID` VARCHAR(32) NOT NULL,
    `DutyGroupName` VARCHAR(64) NOT NULL,
    `DutyDuty` VARCHAR(8) NOT NULL,
    `DutyState` VARCHAR(32) NOT NULL,
    `DutyDutyDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `DutyStateDT` TIMESTAMP(0) NOT NULL DEFAULT '0000-00-00 00:00:00',
    `UserEmail` VARCHAR(64) NOT NULL,
    `UserName` VARCHAR(64) NOT NULL,

    INDEX `idx_dutyjourney_dutydt`(`DutyDutyDT` ASC),
    INDEX `idx_dutyjourney_group_id`(`DutyGroupUID` ASC, `DutyJourneyId` ASC),
    INDEX `idx_dutyjourney_statedt`(`DutyStateDT` ASC),
    PRIMARY KEY (`DutyJourneyId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkfeed` (
    `FeedId` INTEGER NOT NULL AUTO_INCREMENT,
    `FeedUID` VARCHAR(64) NOT NULL,
    `FeedTitle` VARCHAR(128) NOT NULL,
    `FeedText` VARCHAR(1024) NULL,
    `FeedByEmail` VARCHAR(64) NOT NULL,
    `FeedByName` VARCHAR(32) NULL,
    `FeedDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `FeedFile` VARCHAR(64) NULL,
    `FeedEventCal` VARCHAR(64) NULL,
    `FeedPic` VARCHAR(128) NULL,
    `FeedType` VARCHAR(8) NULL,
    `FeedByProfilePic` VARCHAR(64) NULL,
    `FeedGroupUID` VARCHAR(64) NOT NULL,
    `FeedGroupName` VARCHAR(128) NULL,
    `FeedEventLocation` VARCHAR(64) NULL,
    `FeedEventStartDT` TIMESTAMP(0) NULL DEFAULT CURRENT_TIMESTAMP(0),
    `FeedEventEndDT` TIMESTAMP(0) NULL DEFAULT CURRENT_TIMESTAMP(0),
    `FeedEventDuration` INTEGER NULL,
    `NetStatus` VARCHAR(8) NULL,
    `GLat` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `GLong` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `FeedPic2` VARCHAR(128) NOT NULL DEFAULT 'N/A',
    `FeedPic3` VARCHAR(128) NOT NULL DEFAULT 'N/A',
    `FeedPic4` VARCHAR(128) NOT NULL DEFAULT 'N/A',

    UNIQUE INDEX `FeedUID`(`FeedUID` ASC),
    PRIMARY KEY (`FeedId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkgpstracker` (
    `GPTId` INTEGER NOT NULL AUTO_INCREMENT,
    `GGroupUID` VARCHAR(128) NULL,
    `GEmail` VARCHAR(128) NOT NULL,
    `GName` VARCHAR(128) NULL,
    `GLat` VARCHAR(64) NOT NULL,
    `GLong` VARCHAR(64) NOT NULL,
    `GLatLongDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),

    PRIMARY KEY (`GPTId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkgroup` (
    `GroupId` INTEGER NOT NULL AUTO_INCREMENT,
    `GroupUID` VARCHAR(64) NOT NULL,
    `GroupName` VARCHAR(128) NOT NULL,
    `PatrolGPS` VARCHAR(5) NOT NULL DEFAULT 'YES',
    `PatrolWifi` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `PatrolNFC` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `PatrolQR` VARCHAR(5) NOT NULL DEFAULT 'YES',
    `GroupOwner` VARCHAR(64) NOT NULL,
    `LicNumCP` INTEGER NOT NULL DEFAULT 10,
    `LicNumUser` INTEGER NOT NULL DEFAULT 25,
    `LicNumPatrol` INTEGER NOT NULL DEFAULT 10,
    `LicWifi` VARCHAR(5) NOT NULL DEFAULT 'YES',
    `LicQR` VARCHAR(5) NOT NULL DEFAULT 'YES',
    `LicNFC` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `LicGPS` VARCHAR(5) NOT NULL DEFAULT 'YES',
    `IdlePatrolMins` INTEGER NOT NULL DEFAULT 60,
    `LastPatrolWarning` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `IsPatrolWarning` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `SubsStart` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `SubsEnd` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `Status` VARCHAR(8) NOT NULL DEFAULT 'TRIAL',
    `Address` VARCHAR(128) NOT NULL DEFAULT 'World',
    `EmailPatrolAlert` VARCHAR(4) NOT NULL DEFAULT 'NO',
    `EmailAlertAddr` VARCHAR(128) NULL,
    `CustomerID` VARCHAR(16) NOT NULL DEFAULT 'T9957',
    `Features` VARCHAR(128) NOT NULL DEFAULT 'PATROL;GUEST;',
    `TargetPatrolPerDay` INTEGER NOT NULL DEFAULT 12,
    `TarPatDuraMin` INTEGER NOT NULL DEFAULT 30,
    `TarPatCntMin` INTEGER NOT NULL DEFAULT 10,
    `GroupLogo` VARCHAR(64) NULL,
    `WaitCP` INTEGER NOT NULL DEFAULT 0,
    `MaxTripMins` INTEGER NOT NULL DEFAULT 7,
    `PhotoMarker` VARCHAR(5) NOT NULL DEFAULT 'YES',
    `AttMode` VARCHAR(5) NOT NULL DEFAULT 'MAX',

    UNIQUE INDEX `GroupName`(`GroupName` ASC),
    UNIQUE INDEX `unq_tkgroup_groupuid`(`GroupUID` ASC),
    PRIMARY KEY (`GroupId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkguest` (
    `GuestId` INTEGER NOT NULL AUTO_INCREMENT,
    `GuestUID` VARCHAR(64) NULL,
    `GuestGroupUID` VARCHAR(64) NULL,
    `GuestName` VARCHAR(64) NULL,
    `GuestIdentity` VARCHAR(64) NULL,
    `GuestPhoto` VARCHAR(64) NULL,
    `GuestDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `GuestPurpose` VARCHAR(128) NULL,
    `GuestToUserEmail` VARCHAR(32) NULL,
    `GuestOutDT` TIMESTAMP(0) NULL DEFAULT CURRENT_TIMESTAMP(0),
    `GuestCategory` VARCHAR(32) NULL,
    `GuestIdentityType` VARCHAR(16) NULL,
    `QRUID` VARCHAR(64) NOT NULL,
    `GuestNumber` VARCHAR(16) NULL,
    `GuestAddress` VARCHAR(512) NULL,
    `GuestCreatorEmail` VARCHAR(32) NULL,
    `GuestCreatorName` VARCHAR(32) NULL,
    `GuestDestination` VARCHAR(128) NOT NULL,
    `GuestType` VARCHAR(64) NOT NULL,
    `GuestCheckOutByEmail` VARCHAR(32) NULL,
    `GuestAckDT` TIMESTAMP(0) NULL DEFAULT '0000-00-00 00:00:00',

    UNIQUE INDEX `GuestId`(`GuestId` ASC),
    UNIQUE INDEX `GuestUID`(`GuestUID` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkiuran` (
    `IuranId` INTEGER NOT NULL AUTO_INCREMENT,
    `Email` VARCHAR(64) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `StartMonth` INTEGER NOT NULL DEFAULT 1,
    `StartYear` INTEGER NOT NULL DEFAULT 2000,
    `IuranPayment` INTEGER NOT NULL,

    PRIMARY KEY (`IuranId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkiuranpayment` (
    `IuranPaymentId` INTEGER NOT NULL AUTO_INCREMENT,
    `Email` VARCHAR(64) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `PaymentForMonth` INTEGER NOT NULL,
    `PaymentForYear` INTEGER NOT NULL,
    `PaymentDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `PaymentAmount` INTEGER NOT NULL,
    `Status` VARCHAR(16) NOT NULL DEFAULT 'WAITING',
    `PaymentFor` VARCHAR(64) NULL,
    `order_id` VARCHAR(64) NOT NULL,

    UNIQUE INDEX `PaymentForMonth`(`PaymentForMonth` ASC, `PaymentForYear` ASC),
    PRIMARY KEY (`IuranPaymentId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkmasterguesttype` (
    `GuestTypeId` INTEGER NOT NULL AUTO_INCREMENT,
    `GuestTypeUID` VARCHAR(64) NOT NULL,
    `GuestTypeGroupUID` VARCHAR(64) NOT NULL,
    `GuestTypeName` VARCHAR(64) NOT NULL,

    UNIQUE INDEX `GuestTypeUID`(`GuestTypeUID` ASC, `GuestTypeGroupUID` ASC),
    PRIMARY KEY (`GuestTypeId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkmasterstate` (
    `StateId` INTEGER NOT NULL AUTO_INCREMENT,
    `StateUID` VARCHAR(64) NOT NULL,
    `StateGroupUID` VARCHAR(64) NOT NULL,
    `StateName` VARCHAR(64) NOT NULL,
    `StateLat` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `StateLong` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `StateRadius` INTEGER NOT NULL DEFAULT 20,

    UNIQUE INDEX `StateUID`(`StateUID` ASC, `StateGroupUID` ASC),
    PRIMARY KEY (`StateId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tknotification` (
    `NotiId` INTEGER NOT NULL AUTO_INCREMENT,
    `NotiUID` VARCHAR(64) NOT NULL,
    `NotiUserEmail` VARCHAR(128) NOT NULL,
    `NotiTitle` VARCHAR(128) NOT NULL,
    `NotiMessage` VARCHAR(256) NOT NULL,
    `NotiType` VARCHAR(32) NOT NULL,
    `NotiRead` BOOLEAN NULL DEFAULT false,
    `NotiDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `NotiActionData` LONGTEXT NULL,

    UNIQUE INDEX `NotiUID`(`NotiUID` ASC),
    INDEX `idx_notification_dedup`(`NotiUserEmail` ASC, `NotiType` ASC, `NotiDT` ASC),
    INDEX `idx_notification_user_dt`(`NotiUserEmail` ASC, `NotiDT` ASC),
    INDEX `idx_notification_user_read`(`NotiUserEmail` ASC, `NotiRead` ASC),
    PRIMARY KEY (`NotiId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkpatrolevent` (
    `PatEventId` INTEGER NOT NULL AUTO_INCREMENT,
    `PatEventUID` VARCHAR(64) NOT NULL,
    `PatEventGroupUID` VARCHAR(64) NOT NULL,
    `PatEventGroupName` VARCHAR(128) NOT NULL,
    `PatEventGuardName` VARCHAR(64) NOT NULL,
    `PatEventGuardEmail` VARCHAR(32) NOT NULL,
    `PatEventDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `PatEventNFC` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `PatEventQR` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `PatEventWIFI` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `PatEventGPS` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `TotalPatCPNum` INTEGER NOT NULL DEFAULT 0,
    `TotalPatCPPercent` VARCHAR(10) NOT NULL DEFAULT '0',
    `TotalTripMin` INTEGER NOT NULL DEFAULT 0,
    `TotalTargetCP` INTEGER NOT NULL DEFAULT 0,
    `Point` VARCHAR(6) NOT NULL DEFAULT 'BAD',
    `TargetDuraMin` INTEGER NOT NULL DEFAULT 30,

    PRIMARY KEY (`PatEventId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkpatroljourney` (
    `PatJourId` INTEGER NOT NULL AUTO_INCREMENT,
    `PatJourUID` VARCHAR(64) NOT NULL,
    `PatJourEventUID` VARCHAR(64) NOT NULL,
    `PatJourGuardName` VARCHAR(128) NOT NULL,
    `PatJourGuardEmail` VARCHAR(64) NOT NULL,
    `PatJourDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `PatJourGPSLat` VARCHAR(32) NOT NULL,
    `PatJourGPSLong` VARCHAR(32) NOT NULL,
    `PatJourCheckPointName` VARCHAR(64) NOT NULL,
    `PatJourNearestWifiName` VARCHAR(64) NOT NULL,
    `PatJourCheckPointType` VARCHAR(16) NOT NULL,
    `PatJourNFCID` VARCHAR(32) NULL,
    `PatJourQRID` VARCHAR(64) NULL,
    `PatJourNotes` VARCHAR(256) NULL,
    `Cam1` VARCHAR(64) NULL,
    `Cam2` VARCHAR(64) NULL,

    UNIQUE INDEX `PatJourUID`(`PatJourUID` ASC),
    PRIMARY KEY (`PatJourId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkreporttemplate` (
    `TmplId` INTEGER UNSIGNED NOT NULL AUTO_INCREMENT,
    `TmplGroupUID` VARCHAR(64) NOT NULL,
    `TmplKataPengantar` TEXT NULL,
    `TmplMaksud` TEXT NULL,
    `TmplTujuan` TEXT NULL,
    `TmplPenutup` TEXT NULL,
    `TmplSignerName` VARCHAR(100) NULL,
    `TmplSignerTitle` VARCHAR(100) NULL,
    `TmplSignaturePath` VARCHAR(255) NULL,
    `TmplCoverPhoto1Path` VARCHAR(255) NULL,
    `TmplCoverPhoto2Path` VARCHAR(255) NULL,
    `TmplLogoPath` VARCHAR(255) NULL,
    `TmplCity` VARCHAR(100) NULL DEFAULT 'Jakarta',
    `TmplCreatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `TmplUpdatedAt` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `TmplName` VARCHAR(100) NOT NULL DEFAULT 'Default',
    `TmplLogoData` MEDIUMTEXT NULL,
    `TmplCoverPhoto1Data` MEDIUMTEXT NULL,
    `TmplCoverPhoto2Data` MEDIUMTEXT NULL,
    `TmplSignatureData` MEDIUMTEXT NULL,

    UNIQUE INDEX `tkreporttemplate_group_name_unique`(`TmplGroupUID` ASC, `TmplName` ASC),
    PRIMARY KEY (`TmplId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkrpatrolhourly` (
    `PatHourlyId` INTEGER NOT NULL AUTO_INCREMENT,
    `PatHourlyGroupUID` VARCHAR(64) NOT NULL,
    `PatHourlyGroupName` VARCHAR(128) NOT NULL,
    `PatHourlyDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `PatHourMins0` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins1` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins2` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins3` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins4` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins5` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins6` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins7` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins8` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins9` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins10` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins11` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins12` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins13` INTEGER NULL DEFAULT 0,
    `PatHourMins14` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins15` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins16` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins17` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins18` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins19` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins20` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins21` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins22` INTEGER NOT NULL DEFAULT 0,
    `PatHourMins23` INTEGER NOT NULL DEFAULT 0,
    `FinalMinutes` INTEGER NOT NULL DEFAULT 0,

    UNIQUE INDEX `PatHourlyGroupUID`(`PatHourlyGroupUID` ASC, `PatHourlyDT` ASC),
    PRIMARY KEY (`PatHourlyId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkrpatuserhourly` (
    `PatUserHourlyId` INTEGER NOT NULL AUTO_INCREMENT,
    `PatUserHourlyGroupUID` VARCHAR(64) NOT NULL,
    `PatUserHourlyGroupName` VARCHAR(128) NOT NULL,
    `PatUserHourlyDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `PatUserData0` VARCHAR(256) NULL,
    `PatUserData1` VARCHAR(256) NULL,
    `PatUserData2` VARCHAR(256) NULL,
    `PatUserData3` VARCHAR(256) NULL,
    `PatUserData4` VARCHAR(256) NULL,
    `PatUserData5` VARCHAR(256) NULL,
    `PatUserData6` VARCHAR(256) NULL,
    `PatUserData7` VARCHAR(256) NULL,
    `PatUserData8` VARCHAR(256) NULL,
    `PatUserData9` VARCHAR(256) NULL,
    `PatUserData10` VARCHAR(256) NULL,
    `PatUserData11` VARCHAR(256) NULL,
    `PatUserData12` VARCHAR(256) NULL,
    `PatUserData13` VARCHAR(256) NULL,
    `PatUserData14` VARCHAR(256) NULL,
    `PatUserData15` VARCHAR(256) NULL,
    `PatUserData16` VARCHAR(256) NULL,
    `PatUserData17` VARCHAR(256) NULL,
    `PatUserData18` VARCHAR(256) NULL,
    `PatUserData19` VARCHAR(256) NULL,
    `PatUserData20` VARCHAR(256) NULL,
    `PatUserData21` VARCHAR(256) NULL,
    `PatUserData22` VARCHAR(256) NULL,
    `PatUserData23` VARCHAR(256) NULL,
    `FinalNames` INTEGER NOT NULL DEFAULT 0,

    UNIQUE INDEX `PatUserHourlyGroupUID`(`PatUserHourlyGroupUID` ASC, `PatUserHourlyDT` ASC),
    PRIMARY KEY (`PatUserHourlyId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkschedule` (
    `SchedId` INTEGER NOT NULL AUTO_INCREMENT,
    `SchedShiftUID` VARCHAR(32) NOT NULL,
    `SDate` INTEGER NOT NULL,
    `SMonth` INTEGER NOT NULL,
    `SYear` INTEGER NOT NULL,
    `SFriendEmail` VARCHAR(64) NOT NULL,
    `SFriendName` VARCHAR(64) NOT NULL,
    `SGroupUID` VARCHAR(32) NOT NULL,
    `SchedDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `LateMins` INTEGER NOT NULL DEFAULT 0,
    `ACheckin` VARCHAR(24) NULL DEFAULT 'N/A',
    `ACheckout` VARCHAR(24) NULL DEFAULT 'N/A',

    UNIQUE INDEX `SchedShiftUID`(`SchedShiftUID` ASC, `SFriendEmail` ASC, `SDate` ASC, `SMonth` ASC, `SYear` ASC),
    PRIMARY KEY (`SchedId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkshift` (
    `ShiftId` INTEGER NOT NULL AUTO_INCREMENT,
    `ShiftName` VARCHAR(128) NOT NULL,
    `ShiftTimeStart` INTEGER NOT NULL,
    `ShiftTimeStart_mm` INTEGER NOT NULL DEFAULT 0,
    `ShiftTimeStop` INTEGER NOT NULL,
    `ShiftTimeStop_mm` INTEGER NOT NULL DEFAULT 0,
    `ShiftDesc` VARCHAR(128) NULL DEFAULT 'N/A',
    `ShiftGroupUID` VARCHAR(64) NOT NULL,
    `ShiftColor` VARCHAR(16) NOT NULL,
    `ShiftUID` VARCHAR(64) NOT NULL,
    `LocName` VARCHAR(128) NOT NULL DEFAULT 'N/A',
    `LocLat` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `LocLong` VARCHAR(32) NOT NULL DEFAULT 'N/A',
    `LocRadius` INTEGER NOT NULL DEFAULT 0,

    UNIQUE INDEX `ShiftUID`(`ShiftUID` ASC),
    PRIMARY KEY (`ShiftId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkuser` (
    `UserId` INTEGER NOT NULL AUTO_INCREMENT,
    `UserUID` VARCHAR(64) NOT NULL,
    `UserEmail` VARCHAR(128) NOT NULL,
    `UserName` VARCHAR(128) NOT NULL,
    `Password` VARCHAR(64) NOT NULL,
    `FCMID` VARCHAR(512) NOT NULL DEFAULT 'none',
    `InstallId` VARCHAR(512) NOT NULL DEFAULT '1.0.2.27',
    `UserProfilePic` VARCHAR(64) NULL,
    `UserAddress` VARCHAR(128) NULL,
    `UserTitle` VARCHAR(32) NULL,
    `UserRT` VARCHAR(6) NULL,
    `UserRW` VARCHAR(6) NULL,
    `UserDesc` VARCHAR(128) NULL,
    `UserState` VARCHAR(32) NULL,
    `UserStateDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `UserLoc` VARCHAR(64) NULL,
    `UserLocDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `UserDuty` VARCHAR(16) NOT NULL DEFAULT 'OFF DUTY',
    `UserDutyDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `NotiGuestEmail` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `NotiGuestEmailAddr` VARCHAR(64) NULL,
    `Blok` VARCHAR(16) NULL,
    `BlokNumber` INTEGER NOT NULL DEFAULT 0,
    `HomeNumber` INTEGER NOT NULL DEFAULT 0,
    `HomeLetter` VARCHAR(4) NULL,
    `MyLat` VARCHAR(64) NULL DEFAULT 'N/A',
    `MyLong` VARCHAR(64) NULL DEFAULT 'N/A',
    `MyLatLongDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `FaceRegistered` VARCHAR(5) NOT NULL DEFAULT 'NO',

    UNIQUE INDEX `Email`(`UserEmail` ASC),
    PRIMARY KEY (`UserId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkusergroup` (
    `UserGroupId` INTEGER NOT NULL AUTO_INCREMENT,
    `Email` VARCHAR(64) NOT NULL,
    `GroupUID` VARCHAR(64) NOT NULL,
    `AllowAdmin` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `AllowPatrol` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `Category` VARCHAR(16) NOT NULL DEFAULT 'USER',
    `AllowMonPatrol` VARCHAR(5) NOT NULL DEFAULT 'NO',
    `Status` VARCHAR(12) NOT NULL DEFAULT 'JOIN_WAIT',
    `JoinDate` DATETIME(0) NULL DEFAULT CURRENT_TIMESTAMP(0),

    UNIQUE INDEX `UserUID`(`GroupUID` ASC, `Email` ASC),
    PRIMARY KEY (`UserGroupId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `tkweeklyreport` (
    `WeeklyId` INTEGER NOT NULL AUTO_INCREMENT,
    `GroupUID` VARCHAR(64) NOT NULL,
    `ExecDT` TIMESTAMP(0) NOT NULL DEFAULT CURRENT_TIMESTAMP(0),
    `FileName` VARCHAR(64) NOT NULL,
    `PDF_GEN` VARCHAR(4) NOT NULL DEFAULT 'NO',
    `MAIL_SENT` VARCHAR(4) NOT NULL DEFAULT 'NO',
    `MAIL_ADDR` VARCHAR(64) NOT NULL,

    PRIMARY KEY (`WeeklyId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `trprive` (
    `TrPriveId` INTEGER NOT NULL AUTO_INCREMENT,
    `PriveId` VARCHAR(64) NOT NULL,
    `TkGroupUID` VARCHAR(64) NOT NULL,

    INDEX `idx_triprive_priveid`(`PriveId` ASC),
    UNIQUE INDEX `unique_prive_tkgroup`(`PriveId` ASC, `TkGroupUID` ASC),
    PRIMARY KEY (`TrPriveId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- CreateTable
CREATE TABLE `winquiry` (
    `InquiryId` INTEGER NOT NULL AUTO_INCREMENT,
    `Name` VARCHAR(64) NOT NULL,
    `Email` VARCHAR(64) NOT NULL,
    `Phone` VARCHAR(32) NOT NULL,
    `Subject` VARCHAR(128) NOT NULL,
    `Message` VARCHAR(256) NOT NULL,

    PRIMARY KEY (`InquiryId` ASC)
) DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- AddForeignKey
ALTER TABLE `chatmessages` ADD CONSTRAINT `chatmessages_conversation_id_foreign` FOREIGN KEY (`conversation_id`) REFERENCES `chatconversations`(`id`) ON DELETE CASCADE ON UPDATE NO ACTION;


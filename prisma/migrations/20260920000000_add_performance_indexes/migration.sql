SET @OLD_SQL_MODE=@@SQL_MODE, SQL_MODE='';

-- CreateIndex
CREATE INDEX `idx_tkfeed_group_dt` ON `tkfeed`(`FeedGroupUID`, `FeedDT`);

-- CreateIndex
CREATE INDEX `idx_tkguest_group_dt` ON `tkguest`(`GuestGroupUID`, `GuestDT`);

-- CreateIndex
CREATE INDEX `idx_tkusergroup_email` ON `tkusergroup`(`Email`);

SET SQL_MODE=@OLD_SQL_MODE;

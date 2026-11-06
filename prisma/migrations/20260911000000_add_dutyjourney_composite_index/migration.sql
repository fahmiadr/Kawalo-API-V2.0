SET @OLD_SQL_MODE=@@SQL_MODE, SQL_MODE='';

-- CreateIndex
CREATE INDEX `idx_dutyjourney_composite` ON `tkdutyjourney`(`DutyGroupUID`, `DutyDutyDT`, `UserEmail`);

SET SQL_MODE=@OLD_SQL_MODE;

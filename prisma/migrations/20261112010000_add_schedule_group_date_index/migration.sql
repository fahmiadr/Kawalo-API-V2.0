-- Serves the group/date range queries of schedule management (MySQL has no IF NOT EXISTS here).
CREATE INDEX `idx_tkschedule_group_ymd` ON `tkschedule` (`SGroupUID`, `SYear`, `SMonth`, `SDate`);

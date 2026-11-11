-- AlterTable: free-text host name typed by the guard. `GuestToUserEmail` stays for hosts picked
-- from group members (it drives acknowledge authorisation). Plain ALTER: runs on MariaDB and MySQL.
ALTER TABLE `tkguest` ADD COLUMN `GuestToName` VARCHAR(128) NULL;

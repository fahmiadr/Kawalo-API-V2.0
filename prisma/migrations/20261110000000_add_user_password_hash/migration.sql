-- AlterTable: Argon2id PHC string, filled lazily at the first successful login.
-- `Password` stays untouched for the other systems that still read it. NULL means "not hashed yet".
ALTER TABLE `tkuser` ADD COLUMN IF NOT EXISTS `PasswordHash` VARCHAR(255) NULL;

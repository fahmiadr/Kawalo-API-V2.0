-- AlterTable: loudness bars of a recorded PTT turn (48 bytes, 0..255), written by the recorder when
-- the turn is saved. NULL for turns recorded before this migration. Plain ALTER: runs on MariaDB and MySQL.
ALTER TABLE `tkpttrecording`
    ADD COLUMN `Waveform` VARBINARY(48) NULL;

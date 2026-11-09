-- Deterministic dummy data for the parity harness. Loaded into BOTH databases right after a TRUNCATE
-- (so AUTO_INCREMENT restarts at 1). No real data; every password is the plaintext `Rahasia123!`
-- because the legacy auth compares plaintext. Seeded UIDs deliberately avoid the UUIDv7 / UUIDv4
-- shape so the normalizer keeps them literal while runtime-generated ids become placeholders.
SET NAMES utf8mb4;
SET sql_mode = '';
SET time_zone = '+00:00';

-- ---------------------------------------------------------------- groups
INSERT INTO tkgroup (GroupId, GroupUID, GroupName, GroupOwner, Features, GroupLogo, Status) VALUES
 (1, 'grp-alpha', 'Perumahan Alpha', 'owner-alpha', 'PATROL;GUEST;INCIDENT;', 'https://cdn.example.test/logo/alpha.png', 'ACTIVE'),
 (2, 'grp-beta',  'Kompleks Beta',   'owner-beta',  'PATROL; GUEST ;;N/A', NULL, 'TRIAL'),
 (3, 'grp-gamma', 'Klaster Gamma',   'owner-gamma', 'GUEST;', 'N/A', 'ACTIVE');

-- ---------------------------------------------------------------- users
INSERT INTO tkuser (UserId, UserUID, UserEmail, UserName, Password, UserProfilePic, UserDuty) VALUES
 (1,  'u-1',  'admin@alpha.test',      'Andi Admin',        'Rahasia123!', 'https://cdn.example.test/p/andi.jpg', 'ON DUTY'),
 (2,  'u-2',  'guard1@alpha.test',     'Budi Penjaga',      'Rahasia123!', 'https://cdn.example.test/p/budi.jpg', 'ON DUTY'),
 (3,  'u-3',  'guard2@alpha.test',     'Citra Penjaga',     'Rahasia123!', NULL, 'OFF DUTY'),
 (4,  'u-4',  'resident@alpha.test',   'Dewi Warga',        'Rahasia123!', 'https://cdn.example.test/p/dewi.jpg', 'OFF DUTY'),
 (5,  'u-5',  'resident2@alpha.test',  'eko warga',         'Rahasia123!', NULL, 'OFF DUTY'),
 (6,  'u-6',  'resident3@alpha.test',  'Álvaro Ñandú',      'Rahasia123!', NULL, 'OFF DUTY'),
 (7,  'u-7',  'pending@alpha.test',    'Fajar Menunggu',    'Rahasia123!', NULL, 'OFF DUTY'),
 (8,  'u-8',  'admin@beta.test',       'Gita Admin',        'Rahasia123!', NULL, 'ON DUTY'),
 (9,  'u-9',  'guard@beta.test',       'Hadi Beta',         'Rahasia123!', NULL, 'ON DUTY'),
 (10, 'u-10', 'nogroup@test.id',       'Indra Tanpa Grup',  'Rahasia123!', NULL, 'OFF DUTY'),
 (11, 'u-11', 'multi@test.id',         'Kiki Multi',        'Rahasia123!', NULL, 'OFF DUTY'),
 (12, 'u-12', 'supervisor@alpha.test', 'Lina Supervisor',   'Rahasia123!', NULL, 'ON DUTY'),
 (13, 'u-13', 'admin2@alpha.test',     'Maman Admin Dua',   'Rahasia123!', NULL, 'OFF DUTY'),
 (14, 'u-14', 'ghost@alpha.test',      'N/A',               'Rahasia123!', NULL, 'OFF DUTY'),
 (15, 'u-15', 'Mixed.Case@Alpha.test', 'Mira Campur',       'Rahasia123!', NULL, 'OFF DUTY'),
 (20, 'u-20', 'lock1@gamma.test',      'Lock Satu',         'Rahasia123!', NULL, 'OFF DUTY'),
 (21, 'u-21', 'lock2@gamma.test',      'Lock Dua',          'Rahasia123!', NULL, 'OFF DUTY'),
 (22, 'u-22', 'lock3@gamma.test',      'Lock Tiga',         'Rahasia123!', NULL, 'OFF DUTY'),
 (23, 'u-23', 'refresh1@gamma.test',   'Refresh Satu',      'Rahasia123!', NULL, 'OFF DUTY'),
 (24, 'u-24', 'refresh2@gamma.test',   'Refresh Dua',       'Rahasia123!', NULL, 'OFF DUTY'),
 (25, 'u-25', 'refresh3@gamma.test',   'Refresh Tiga',      'Rahasia123!', NULL, 'OFF DUTY'),
 (26, 'u-26', 'refresh4@gamma.test',   'Refresh Empat',     'Rahasia123!', NULL, 'OFF DUTY'),
 (27, 'u-27', 'switch1@gamma.test',    'Switch Satu',       'Rahasia123!', NULL, 'OFF DUTY'),
 (28, 'u-28', 'logout1@gamma.test',    'Logout Satu',       'Rahasia123!', NULL, 'OFF DUTY'),
 (29, 'u-29', 'logout2@gamma.test',    'Logout Dua',        'Rahasia123!', NULL, 'OFF DUTY'),
 (30, 'u-30', 'login1@gamma.test',     'Login Satu',        'Rahasia123!', NULL, 'OFF DUTY'),
 (31, 'u-31', 'login2@gamma.test',     'Login Dua',         'Rahasia123!', NULL, 'OFF DUTY'),
 (32, 'u-32', 'iplimit@gamma.test',    'Ip Limit',          'Rahasia123!', NULL, 'OFF DUTY');

-- ---------------------------------------------------------------- memberships (row order = PK order = default active-group order)
INSERT INTO tkusergroup (Email, GroupUID, AllowAdmin, AllowPatrol, Category, AllowMonPatrol, Status) VALUES
 ('admin@alpha.test',      'grp-alpha', 'YES', 'YES', 'ADMIN', 'YES', 'JOINED'),
 ('admin@alpha.test',      'grp-beta',  'no',  'NO',  'user',  'NO',  'JOINED'),
 ('admin@alpha.test',      'grp-gamma', 'NO',  'YES', 'GUARD', 'NO',  'JOIN_WAIT'),
 ('guard1@alpha.test',     'grp-alpha', 'NO',  'YES', 'GUARD', 'NO',  'JOINED'),
 ('guard2@alpha.test',     'grp-alpha', 'NO',  'YES', 'guard', 'NO',  'JOINED'),
 ('resident@alpha.test',   'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('resident2@alpha.test',  'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('resident3@alpha.test',  'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('pending@alpha.test',    'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOIN_WAIT'),
 ('admin@beta.test',       'grp-beta',  'YES', 'YES', 'ADMIN', 'YES', 'JOINED'),
 ('guard@beta.test',       'grp-beta',  'NO',  'YES', 'GUARD', 'NO',  'JOINED'),
 ('multi@test.id',         'grp-gamma', 'NO',  'NO',  'ADMIN', 'NO',  'JOIN_WAIT'),
 ('multi@test.id',         'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('multi@test.id',         'grp-beta',  'NO',  'YES', 'GUARD', 'NO',  'JOINED'),
 ('supervisor@alpha.test', 'grp-alpha', 'YES', 'YES', 'GUARD', 'YES', 'JOINED'),
 ('admin2@alpha.test',     'grp-alpha', 'NO',  'YES', 'ADMIN', 'NO',  'JOINED'),
 ('ghost@alpha.test',      'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('Mixed.Case@Alpha.test', 'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('orphan@alpha.test',     'grp-alpha', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('lock1@gamma.test',      'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('lock2@gamma.test',      'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('lock3@gamma.test',      'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('refresh1@gamma.test',   'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('refresh2@gamma.test',   'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('refresh3@gamma.test',   'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('refresh4@gamma.test',   'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('switch1@gamma.test',    'grp-gamma', 'YES', 'NO',  'ADMIN', 'NO',  'JOINED'),
 ('switch1@gamma.test',    'grp-alpha', 'NO',  'YES', 'GUARD', 'NO',  'JOINED'),
 ('switch1@gamma.test',    'grp-beta',  'NO',  'NO',  'USER',  'NO',  'JOIN_WAIT'),
 ('logout1@gamma.test',    'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('logout2@gamma.test',    'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('login1@gamma.test',     'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('login2@gamma.test',     'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED'),
 ('iplimit@gamma.test',    'grp-gamma', 'NO',  'NO',  'USER',  'NO',  'JOINED');

-- ---------------------------------------------------------------- guest master types
INSERT INTO tkmasterguesttype (GuestTypeId, GuestTypeUID, GuestTypeGroupUID, GuestTypeName) VALUES
 (1, 'gt-a-umum',   'grp-alpha', 'Tamu Umum'),
 (2, 'gt-a-kurir',  'grp-alpha', 'Kurir'),
 (3, 'gt-a-ojek',   'grp-alpha', 'Ojek Online'),
 (4, 'gt-a-kontr',  'grp-alpha', 'Kontraktor'),
 (5, 'gt-b-umum',   'grp-beta',  'Tamu Umum'),
 (6, 'gt-b-vip',    'grp-beta',  'Tamu VIP');

-- ---------------------------------------------------------------- guests (alpha: 12 rows, beta: 3 rows)
INSERT INTO tkguest (GuestId, GuestUID, GuestGroupUID, GuestName, GuestNumber, GuestIdentity, GuestIdentityType, GuestPhoto, GuestDT, GuestAckDT, GuestOutDT, GuestPurpose, GuestDestination, GuestToUserEmail, GuestCreatorEmail, GuestCreatorName, GuestCategory, QRUID, GuestAddress, GuestType, GuestCheckOutByEmail) VALUES
 (1,  'guest-seed-01', 'grp-alpha', 'Rudi Tamu',       'B1234XYZ', '3201010101010001', 'KTP',    'https://cdn.example.test/g/1.jpg', '2026-03-01 08:00:00', NULL, NULL, 'Antar paket', 'Blok A1', 'resident@alpha.test', 'guard1@alpha.test', 'Budi Penjaga', 'Umum', 'QR-A-0001', 'Jl. Melati 1', 'Tamu Umum', NULL),
 (2,  'guest-seed-02', 'grp-alpha', 'Siti Kurir',      'D5678AB',  NULL,               NULL,     NULL,                               '2026-03-01 09:30:00', '2026-03-01 09:35:00', NULL, 'Paket Tokopedia', 'Blok B2', 'resident2@alpha.test', 'guard2@alpha.test', 'Citra Penjaga', 'Umum', 'QR-A-0002', NULL, 'Kurir', NULL),
 (3,  'guest-seed-03', 'grp-alpha', 'Tono Kontraktor', 'F9012CD',  '3201010101010003', 'SIM',    NULL,                               '2026-03-02 07:00:00', '2026-03-02 07:10:00', '2026-03-02 16:45:00', 'Renovasi dapur', 'Blok C3', 'resident3@alpha.test', 'guard1@alpha.test', 'Budi Penjaga', 'Umum', 'QR-A-0003', 'Jl. Mawar 5', 'Kontraktor', 'guard1@alpha.test'),
 (4,  'guest-seed-04', 'grp-alpha', 'Uci Ojek',        'N/A',      'null',             'LAINNYA', NULL,                              '2026-03-02 12:00:00', NULL, '2026-03-02 12:20:00', 'N/A', 'Blok A1', 'resident@alpha.test', 'admin@alpha.test', 'Andi Admin', 'Umum', 'QR-A-0004', '  ', 'Ojek Online', 'admin@alpha.test'),
 (5,  'guest-seed-05', 'grp-alpha', 'Vina Paket',      'B1111AA',  NULL,               NULL,     NULL,                               '2026-03-03 10:00:00', NULL, NULL, 'Paket 100% fragile_', 'Blok D4', 'resident2@alpha.test', 'guard1@alpha.test', 'Budi Penjaga', 'Umum', 'QR-A-0005', NULL, 'Kurir', NULL),
 (6,  'guest-seed-06', 'grp-alpha', 'Wawan Zero',      'B2222BB',  NULL,               NULL,     NULL,                               '2026-03-03 11:00:00', '0000-00-00 00:00:00', NULL, 'Kunjungan keluarga', 'Blok A1', 'resident@alpha.test', 'guard2@alpha.test', 'Citra Penjaga', 'Umum', 'QR-A-0006', NULL, 'Tamu Umum', NULL),
 (7,  'guest-seed-07', 'grp-alpha', 'Xena Tamu',       NULL,       NULL,               NULL,     NULL,                               '2026-03-04 08:15:00', NULL, NULL, NULL, 'Blok E5', NULL, 'guard1@alpha.test', 'Budi Penjaga', 'Umum', 'QR-A-0007', NULL, 'Tamu Umum', NULL),
 (8,  'guest-seed-08', 'grp-alpha', 'Yusuf Tamu',      NULL,       NULL,               NULL,     NULL,                               '2026-03-04 09:15:00', '2026-03-04 09:20:00', NULL, 'Rapat RT', 'Balai Warga', 'admin@alpha.test', 'admin@alpha.test', 'Andi Admin', 'Umum', 'QR-A-0008', NULL, 'Tamu Umum', NULL),
 (9,  'guest-seed-09', 'grp-alpha', 'Zaki Tamu',       NULL,       NULL,               NULL,     NULL,                               '2026-03-05 08:00:00', NULL, '2026-03-05 10:00:00', 'Survey', 'Blok A1', 'resident@alpha.test', 'guard2@alpha.test', 'Citra Penjaga', 'Umum', 'QR-A-0009', NULL, 'Tamu Umum', 'guard2@alpha.test'),
 (10, 'guest-seed-10', 'grp-alpha', 'Agus Tamu',       NULL,       NULL,               NULL,     NULL,                               '2026-03-05 08:00:00', NULL, NULL, 'Survey kedua', 'Blok A2', NULL, 'guard1@alpha.test', 'Budi Penjaga', 'Umum', 'QR-A-0010', NULL, 'Tamu Umum', NULL),
 (11, 'guest-seed-11', 'grp-alpha', 'Bela Tamu',       NULL,       NULL,               NULL,     NULL,                               '2026-03-06 08:00:00', NULL, NULL, 'Antar makanan', 'Blok A3', NULL, 'guard2@alpha.test', 'Citra Penjaga', 'Umum', 'QR-A-0011', NULL, 'Ojek Online', NULL),
 (12, 'guest-seed-12', 'grp-alpha', 'Candra Tamu',     NULL,       NULL,               NULL,     NULL,                               '2026-03-07 08:00:00', NULL, NULL, 'Kunjungan', 'Blok A4', NULL, 'guard1@alpha.test', 'Budi Penjaga', 'Umum', '12', NULL, 'Tamu Umum', NULL),
 (13, 'guest-seed-13', 'grp-beta',  'Beta Tamu Satu',  'B7777BT',  NULL,               NULL,     NULL,                               '2026-03-01 08:00:00', NULL, NULL, 'Kunjungan', 'Rumah 1', 'admin@beta.test', 'guard@beta.test', 'Hadi Beta', 'Umum', 'QR-B-0001', NULL, 'Tamu Umum', NULL),
 (14, 'guest-seed-14', 'grp-beta',  'Beta Tamu Dua',   NULL,       NULL,               NULL,     NULL,                               '2026-03-02 08:00:00', NULL, '2026-03-02 09:00:00', 'Kunjungan', 'Rumah 2', NULL, 'admin@beta.test', 'Gita Admin', 'Umum', 'QR-B-0002', NULL, 'Tamu VIP', 'admin@beta.test'),
 (15, 'guest-seed-15', 'grp-beta',  'Rudi Tamu',       NULL,       NULL,               NULL,     NULL,                               '2026-03-03 08:00:00', NULL, NULL, 'Kunjungan', 'Rumah 3', NULL, 'guard@beta.test', 'Hadi Beta', 'Umum', 'QR-B-0003', NULL, 'Tamu VIP', NULL);

-- ---------------------------------------------------------------- feed / information
INSERT INTO tkfeed (FeedId, FeedUID, FeedTitle, FeedText, FeedByEmail, FeedByName, FeedDT, FeedFile, FeedPic, FeedType, FeedByProfilePic, FeedGroupUID, FeedGroupName, FeedEventLocation, FeedEventStartDT, FeedEventEndDT, FeedEventDuration, GLat, GLong, FeedPic2, FeedPic3, FeedPic4) VALUES
 (1, 'feed-seed-01', 'Kerja bakti Minggu',   'Seluruh warga diharapkan hadir',       'admin@alpha.test',  'Andi Admin',    '2026-03-01 06:00:00', NULL, 'https://cdn.example.test/f/1a.jpg', 'EVENT',   NULL, 'grp-alpha', 'Perumahan Alpha', 'Lapangan Utama', '2026-03-08 06:00:00', '2026-03-08 09:00:00', 180, '-6.2000000', '106.8000000', 'https://cdn.example.test/f/1b.jpg', NULL, NULL),
 (2, 'feed-seed-02', 'Pemadaman listrik',    'PLN memadamkan listrik pukul 10.00',   'guard1@alpha.test', NULL,            '2026-03-02 07:30:00', NULL, NULL, 'GENERAL', NULL, 'grp-alpha', NULL, NULL, NULL, NULL, NULL, 'N/A', 'N/A', NULL, NULL, NULL),
 (3, 'feed-seed-03', 'Pencurian helm',       'Dilaporkan hilang di Blok C',          'guard2@alpha.test', 'Citra Penjaga', '2026-03-03 22:10:00', NULL, 'https://cdn.example.test/f/3a.jpg', 'INCIDENT', 'https://cdn.example.test/p/citra.jpg', 'grp-alpha', 'Alpha Lama', 'Blok C', NULL, NULL, NULL, '0', '0', NULL, 'https://cdn.example.test/f/3c.jpg', NULL),
 (4, 'feed-seed-04', 'Selamat datang warga', 'Warga baru telah bergabung',           'admin@alpha.test',  'Andi Admin',    '2026-03-04 08:00:00', 'https://cdn.example.test/f/doc.pdf', NULL, 'JOIN', NULL, 'grp-alpha', 'Perumahan Alpha', NULL, NULL, NULL, NULL, '-6.2100000', '106.8100000', NULL, NULL, NULL),
 (5, 'feed-seed-05', 'Info 100% gratis_',    'Pencarian wildcard % dan _',           'guard1@alpha.test', 'Budi Penjaga',  '2026-03-05 09:00:00', NULL, NULL, 'OTHER',   NULL, 'grp-alpha', 'Perumahan Alpha', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
 (6, 'feed-seed-06', 'Rapat RT bulanan',     'Agenda: iuran, keamanan, kebersihan',  'admin@alpha.test',  'Andi Admin',    '2026-03-05 09:00:00', NULL, NULL, 'EVENT',   NULL, 'grp-alpha', 'Perumahan Alpha', 'Balai Warga', '2026-03-10 19:00:00', '2026-03-10 21:00:00', 120, NULL, NULL, NULL, NULL, NULL),
 (7, 'feed-seed-07', 'Parkir liar',          'Mohon tidak parkir di depan gerbang',  'supervisor@alpha.test', NULL,        '2026-03-06 12:00:00', NULL, NULL, 'GENERAL', NULL, 'grp-alpha', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
 (8, 'feed-seed-08', 'Jadwal ronda',         NULL,                                   'guard1@alpha.test', 'Budi Penjaga',  '2026-03-07 15:00:00', NULL, 'N/A', 'OTHER', NULL, 'grp-alpha', 'Perumahan Alpha', NULL, NULL, NULL, NULL, NULL, NULL, 'https://cdn.example.test/f/8b.jpg', 'https://cdn.example.test/f/8c.jpg', 'https://cdn.example.test/f/8d.jpg'),
 (9, 'feed-seed-09', 'Pengumuman Beta',      'Khusus warga Beta',                    'admin@beta.test',   'Gita Admin',    '2026-03-01 06:00:00', NULL, NULL, 'GENERAL', NULL, 'grp-beta',  'Kompleks Beta', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
 (10,'feed-seed-10', 'Kegiatan Beta',        NULL,                                   'guard@beta.test',   NULL,            '2026-03-02 06:00:00', NULL, NULL, 'EVENT',   NULL, 'grp-beta',  NULL, 'Taman Beta', '2026-03-12 07:00:00', '2026-03-12 08:00:00', 60, NULL, NULL, NULL, NULL, NULL);

-- ---------------------------------------------------------------- shifts
INSERT INTO tkshift (ShiftId, ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, ShiftTimeStop_mm, ShiftDesc, ShiftGroupUID, ShiftColor, ShiftUID) VALUES
 (1, 'Shift Pagi',   7,  0,  15, 0,  'N/A', 'grp-alpha', '#2E7D32', 'shift-a-pagi'),
 (2, 'Shift Malam',  22, 30, 6,  15, 'N/A', 'grp-alpha', '#1565C0', 'shift-a-malam'),
 (3, 'Shift Aneh',   25, 0,  8,  0,  'N/A', 'grp-alpha', '#C62828', 'shift-a-aneh'),
 (4, 'Shift Beta',   8,  0,  16, 0,  'N/A', 'grp-beta',  '#EF6C00', 'shift-b-1');

-- ---------------------------------------------------------------- attendance
INSERT INTO tkabsen (AbsenID, AbsenEmail, AbsenName, AbsenDT, AbsenType, ALat, ALong, ADiffMins, ALocation, AShiftUID, AbsenFunc, AbsenGroupUID, AShStart) VALUES
 (1,  'guard1@alpha.test', 'Budi Lama',      '2026-03-01 06:55:00', 'Check In',        '-6.2000000', '106.8000000', -5,  'Pos Utama',    'shift-a-pagi',  0, 'grp-alpha', '07:00:00'),
 (2,  'guard1@alpha.test', 'Budi Lama',      '2026-03-01 15:02:00', 'Check Out',       '-6.2000000', '106.8000000', 2,   'Pos Utama',    'shift-a-pagi',  0, 'grp-alpha', '07:00:00'),
 (3,  'guard2@alpha.test', 'Citra',          '2026-03-01 22:40:00', 'Masuk Malam',     '0',          '0',           10,  'Pos Belakang', 'shift-a-malam', 0, 'grp-alpha', '22:30:00'),
 (4,  'guard2@alpha.test', 'Citra',          '2026-03-02 06:20:00', 'Pulang Pagi',     'N/A',        'N/A',         5,   'Pos Belakang', 'shift-a-malam', 0, 'grp-alpha', '22:30:00'),
 (5,  'supervisor@alpha.test', 'Lina',       '2026-03-02 08:00:00', 'REGISTER',        '-6.2100000', '106.8100000', 0,   'N/A',          'shift-a-aneh',  0, 'grp-alpha', '08:30:00'),
 (6,  'guard1@alpha.test', 'Budi',           '2026-03-03 07:05:00', 'check in',        '-6.2000000', '106.8000000', 5,   'Pos Utama',    'N/A',           0, 'grp-alpha', '00:00:00'),
 (7,  'unknown@alpha.test','Tanpa Akun',     '2026-03-03 07:10:00', 'Lainnya',         NULL,         NULL,          0,   'Pos Utama',    'shift-tidak-ada', 0, 'grp-alpha', '09:15:00'),
 (8,  'guard2@alpha.test', 'Citra',          '2026-03-04 15:00:00', 'Check Out Sore',  '-6.2000000', '106.8000000', -1,  'Pos Utama',    'shift-a-pagi',  0, 'grp-alpha', '07:00:00'),
 (9,  'guard@beta.test',   'Hadi',           '2026-03-01 08:05:00', 'Check In',        '-6.3000000', '106.9000000', 5,   'Gerbang Beta', 'shift-b-1',     0, 'grp-beta',  '08:00:00'),
 (10, 'guard@beta.test',   'Hadi',           '2026-03-01 16:00:00', 'Check Out',       '-6.3000000', '106.9000000', 0,   'Gerbang Beta', 'shift-b-1',     0, 'grp-beta',  '08:00:00');

-- ---------------------------------------------------------------- duty journeys (operational)
INSERT INTO tkdutyjourney (DutyJourneyId, DutyJourneyUID, DutyGroupUID, DutyGroupName, DutyDuty, DutyState, DutyDutyDT, DutyStateDT, UserEmail, UserName) VALUES
 (1,  'dj-001', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'IDLE',      '2026-03-01 07:00:00', '2026-03-01 07:00:00', 'guard1@alpha.test', 'Budi Penjaga'),
 (2,  'dj-002', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'PATROL',    '2026-03-01 09:00:00', '2026-03-01 09:00:00', 'guard1@alpha.test', 'Budi Penjaga'),
 (3,  'dj-003', 'grp-alpha', 'Perumahan Alpha', 'OFF DUTY', 'IDLE',      '2026-03-01 15:00:00', '2026-03-01 15:00:00', 'guard1@alpha.test', 'Budi Penjaga'),
 (4,  'dj-004', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'IDLE',      '2026-03-01 22:30:00', '2026-03-01 22:30:00', 'guard2@alpha.test', 'Citra Penjaga'),
 (5,  'dj-005', 'grp-alpha', '',                'ON DUTY',  'PATROL',    '2026-03-02 01:00:00', '2026-03-02 01:00:00', 'guard2@alpha.test', ''),
 (6,  'dj-006', 'grp-alpha', 'Perumahan Alpha', 'OFF DUTY', 'IDLE',      '2026-03-02 06:00:00', '2026-03-02 06:00:00', 'guard2@alpha.test', 'Citra Penjaga'),
 (7,  'dj-007', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'IDLE',      '2026-03-02 07:00:00', '0000-00-00 00:00:00', 'guard1@alpha.test', 'Budi Penjaga'),
 (8,  'dj-008', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'PATROL',    '2026-03-03 10:00:00', '2026-03-03 10:00:00', 'supervisor@alpha.test', 'Lina Supervisor'),
 (9,  'dj-009', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'EMERGENCY', '2026-03-03 23:50:00', '2026-03-03 23:50:00', 'guard1@alpha.test', 'Budi Penjaga'),
 (10, 'dj-010', 'grp-alpha', 'Perumahan Alpha', 'ON DUTY',  'IDLE',      '2026-03-04 00:10:00', '2026-03-04 00:10:00', 'guard1@alpha.test', 'Budi Penjaga'),
 (11, 'dj-011', 'grp-beta',  'Kompleks Beta',   'ON DUTY',  'IDLE',      '2026-03-01 08:00:00', '2026-03-01 08:00:00', 'guard@beta.test',   'Hadi Beta'),
 (12, 'dj-012', 'grp-beta',  'Kompleks Beta',   'OFF DUTY', 'IDLE',      '2026-03-01 16:00:00', '2026-03-01 16:00:00', 'guard@beta.test',   'Hadi Beta');

-- ---------------------------------------------------------------- checkpoints and schedules (not read by the API; present for completeness)
INSERT INTO tkcheckpoint (CheckPointId, CheckPointUID, CPGroupUID, CPName, CPType, CPGPSLat, CPGPSLong, CPWifiSSID, CPNFCIdentity, CPQRIdentity, CPWifiMAC, CPDesc) VALUES
 (1, 'cp-a-1', 'grp-alpha', 'Gerbang Utama', 'GPS', '-6.2', '106.8', '', '', '', '', 'Pos 1'),
 (2, 'cp-a-2', 'grp-alpha', 'Gerbang Belakang', 'QR', '-6.21', '106.81', '', '', 'QR-CP-2', '', 'Pos 2');
INSERT INTO tkschedule (SchedId, SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, SFriendName, SGroupUID) VALUES
 (1, 'shift-a-pagi', 1, 3, 2026, 'guard1@alpha.test', 'Budi Penjaga', 'grp-alpha'),
 (2, 'shift-a-malam', 1, 3, 2026, 'guard2@alpha.test', 'Citra Penjaga', 'grp-alpha');

-- ---------------------------------------------------------------- incidents
INSERT INTO tkincident (IncidentUID, GroupUID, Title, Description, Category, GLat, GLong, Photos, Status, CreatedByEmail, CreatedByName, ResolvedByEmail, ResolvedByName, ResolvedAt, DeletedAt, CreatedAt, UpdatedAt) VALUES
 ('inc_00000000-0000-0000-0000-000000000001', 'grp-alpha', 'Lampu jalan mati', 'Lampu di depan Blok A mati total', 'facility', -6.2, 106.8, '["https://cdn.example.test/i/1.jpg"]', 'OPEN', 'guard1@alpha.test', 'Budi Penjaga', NULL, NULL, NULL, NULL, '2026-03-01 07:00:00', '2026-03-01 07:00:00'),
 ('inc_00000000-0000-0000-0000-000000000002', 'grp-alpha', 'Pagar rusak', 'Pagar belakang roboh', 'facility', NULL, NULL, NULL, 'RESOLVED', 'resident@alpha.test', 'Dewi Warga', 'admin@alpha.test', 'Andi Admin', '2026-03-02 10:00:00', NULL, '2026-03-02 08:00:00', '2026-03-02 10:00:00'),
 ('inc_00000000-0000-0000-0000-000000000003', 'grp-alpha', 'Insiden terhapus', NULL, 'general', NULL, NULL, NULL, 'OPEN', 'guard1@alpha.test', 'Budi Penjaga', NULL, NULL, NULL, '2026-03-03 12:00:00', '2026-03-03 09:00:00', '2026-03-03 12:00:00'),
 ('inc_00000000-0000-0000-0000-000000000004', 'grp-alpha', 'Orang mencurigakan', 'N/A', 'security', 0, 0, 'bukan json', 'OPEN', 'guard2@alpha.test', NULL, NULL, NULL, NULL, NULL, '2026-03-04 22:00:00', '2026-03-04 22:00:00'),
 ('inc_00000000-0000-0000-0000-000000000005', 'grp-beta',  'Insiden Beta', 'Hanya milik Beta', 'hazard', -6.3, 106.9, NULL, 'OPEN', 'guard@beta.test', 'Hadi Beta', NULL, NULL, NULL, NULL, '2026-03-01 09:00:00', '2026-03-01 09:00:00');
INSERT INTO tkincidentreply (ReplyUID, IncidentUID, SenderEmail, SenderName, SenderRole, Message, Photos, CreatedAt) VALUES
 ('rpl_00000000-0000-0000-0000-000000000001', 'inc_00000000-0000-0000-0000-000000000001', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', 'Sedang dicek teknisi', NULL, '2026-03-01 07:30:00'),
 ('rpl_00000000-0000-0000-0000-000000000002', 'inc_00000000-0000-0000-0000-000000000001', 'admin@alpha.test', 'Andi Admin', 'ADMIN', 'Teknisi datang besok', '["https://cdn.example.test/r/2.jpg"]', '2026-03-01 08:00:00'),
 ('rpl_00000000-0000-0000-0000-000000000003', 'inc_00000000-0000-0000-0000-000000000002', 'admin@alpha.test', 'Andi Admin', 'ADMIN', 'Sudah diperbaiki', NULL, '2026-03-02 10:00:00');

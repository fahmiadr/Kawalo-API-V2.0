#!/usr/bin/env node
// Seeds kawalo_bench with synthetic, realistic-volume data. Never touches data.sql.
// usage: node tools/bench/seed.mjs [--scale 1]
import { spawn } from 'node:child_process';

const scale = Number(process.argv[process.argv.indexOf('--scale') + 1]) || 1;
const N = {
  guests: Math.round(50_000 * scale),
  feeds: Math.round(20_000 * scale),
  absen: Math.round(100_000 * scale),
  duty: Math.round(60_000 * scale),
};
export const USERS = { g1: 90, g2: 40, g3: 20 }; // user counts per group (ids are contiguous: g1 first)
const TOTAL_USERS = USERS.g1 + USERS.g2 + USERS.g3;

// ~70% of rows in bench-g1 (the group the benchmark authenticates as), 20% g2, 10% g3.
const grp = (n) => `ELT(1 + (${n} MOD 10 >= 7) + (${n} MOD 10 >= 9), 'bench-g1', 'bench-g2', 'bench-g3')`;
const userIdx = (n) => `(1 + ${n} MOD ${TOTAL_USERS})`;
const email = (idx) => `CONCAT('bench-user-', ${idx}, '@bench.test')`;

const sql = `
SET FOREIGN_KEY_CHECKS=0;
SET SESSION sql_mode='';
TRUNCATE tkguest; TRUNCATE tkfeed; TRUNCATE tkabsen; TRUNCATE tkdutyjourney; TRUNCATE tkshift;
TRUNCATE tkusergroup; TRUNCATE tkuser; TRUNCATE tkgroup; TRUNCATE tkmasterguesttype;
TRUNCATE tkincident; TRUNCATE tkincidentreply;

INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner, Status) VALUES
 ('bench-g1','Perumahan Bench 1','bench-user-1@bench.test','ACTIVE'),
 ('bench-g2','Perumahan Bench 2','bench-user-91@bench.test','ACTIVE'),
 ('bench-g3','Perumahan Bench 3','bench-user-131@bench.test','ACTIVE');

INSERT INTO tkuser (UserUID, UserEmail, UserName, Password)
 SELECT CONCAT('u_', seq), ${email('seq')}, CONCAT('Petugas Bench ', seq), 'bench123'
 FROM seq_1_to_${TOTAL_USERS};

INSERT INTO tkusergroup (Email, GroupUID, AllowAdmin, AllowPatrol, Category, AllowMonPatrol, Status)
 SELECT ${email('seq')},
   IF(seq <= ${USERS.g1}, 'bench-g1', IF(seq <= ${USERS.g1 + USERS.g2}, 'bench-g2', 'bench-g3')),
   IF(seq MOD 10 = 0, 'YES', 'NO'), 'YES',
   IF(seq MOD 10 = 0, 'ADMIN', IF(seq MOD 3 = 0, 'USER', 'GUARD')), 'NO', 'JOINED'
 FROM seq_1_to_${TOTAL_USERS};

INSERT INTO tkmasterguesttype (GuestTypeUID, GuestTypeGroupUID, GuestTypeName)
 SELECT CONCAT('gt_', g.u, '_', t.seq), g.u, ELT(t.seq, 'Tamu', 'Kurir', 'Ojek Online', 'Vendor', 'Keluarga')
 FROM (SELECT 'bench-g1' u UNION SELECT 'bench-g2' UNION SELECT 'bench-g3') g, seq_1_to_5 t;

INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStop, ShiftGroupUID, ShiftColor, ShiftUID)
 SELECT ELT(t.seq, 'Pagi', 'Siang', 'Malam'), ELT(t.seq, 6, 14, 22), ELT(t.seq, 14, 22, 6),
        g.u, ELT(t.seq, '#2563EB', '#D97706', '#475569'), CONCAT('sh_', g.u, '_', t.seq)
 FROM (SELECT 'bench-g1' u UNION SELECT 'bench-g2' UNION SELECT 'bench-g3') g, seq_1_to_3 t;

INSERT INTO tkguest (GuestUID, GuestGroupUID, GuestName, GuestIdentity, GuestPhoto, GuestDT, GuestPurpose,
    GuestToUserEmail, GuestOutDT, GuestCategory, GuestIdentityType, QRUID, GuestNumber, GuestAddress,
    GuestCreatorEmail, GuestCreatorName, GuestDestination, GuestType)
 SELECT CONCAT('guest_', seq), ${grp('seq')}, CONCAT('Tamu ', seq),
   LPAD(seq * 7919 MOD 10000000000000000, 16, '3'), CONCAT('https://cdn.bench.test/guest/', seq, '.jpg'),
   NOW() - INTERVAL (seq * 5) MINUTE, ELT(1 + seq MOD 4, 'Bertamu', 'Antar paket', 'Servis AC', 'Rapat RT'),
   ${email(userIdx('seq'))}, IF(seq MOD 3 = 0, NULL, NOW() - INTERVAL (seq * 5) MINUTE + INTERVAL 30 MINUTE),
   'REGULER', ELT(1 + seq MOD 3, 'KTP', 'SIM', 'LAINNYA'), CONCAT('qr_', seq),
   CONCAT('B ', 1000 + seq MOD 9000, ' XY'), CONCAT('Jl. Melati No. ', seq MOD 200),
   ${email(userIdx('seq'))}, CONCAT('Petugas Bench ', ${userIdx('seq')}),
   CONCAT('Blok ', CHAR(65 + seq MOD 8), ' No. ', seq MOD 40), ELT(1 + seq MOD 5, 'Tamu', 'Kurir', 'Ojek Online', 'Vendor', 'Keluarga')
 FROM seq_1_to_${N.guests};

INSERT INTO tkfeed (FeedUID, FeedTitle, FeedText, FeedByEmail, FeedByName, FeedDT, FeedType, FeedGroupUID,
    FeedGroupName, FeedEventLocation, GLat, GLong, FeedPic)
 SELECT CONCAT('feed_', seq), CONCAT('Informasi patroli #', seq),
   CONCAT('Laporan situasi rutin nomor ', seq, '. Semua aman terkendali, tidak ada kejadian menonjol di lokasi.'),
   ${email(userIdx('seq'))}, CONCAT('Petugas Bench ', ${userIdx('seq')}), NOW() - INTERVAL (seq * 13) MINUTE,
   ELT(1 + seq MOD 3, 'GENERAL', 'INCIDENT', 'EVENT'), ${grp('seq')}, 'Perumahan Bench',
   CONCAT('Pos ', 1 + seq MOD 6), '-6.2000', '106.8166', CONCAT('https://cdn.bench.test/feed/', seq, '.jpg')
 FROM seq_1_to_${N.feeds};

INSERT INTO tkabsen (AbsenEmail, AbsenName, AbsenDT, AbsenType, ALat, ALong, ADiffMins, ALocation, AShiftUID,
    AbsenGroupUID, AShStart)
 SELECT ${email(userIdx('seq'))}, CONCAT('Petugas Bench ', ${userIdx('seq')}), NOW() - INTERVAL (seq * 3) MINUTE,
   ELT(1 + seq MOD 2, 'CHECKIN', 'CHECKOUT'), '-6.2000', '106.8166', seq MOD 15, CONCAT('Pos ', 1 + seq MOD 6),
   CONCAT('sh_', ${grp('seq')}, '_', 1 + seq MOD 3), ${grp('seq')}, ELT(1 + seq MOD 3, '06:00:00', '14:00:00', '22:00:00')
 FROM seq_1_to_${N.absen};

INSERT INTO tkdutyjourney (DutyJourneyUID, DutyGroupUID, DutyGroupName, DutyDuty, DutyState, DutyDutyDT,
    DutyStateDT, UserEmail, UserName)
 SELECT CONCAT('dj_', seq), ${grp('seq')}, 'Perumahan Bench', ELT(1 + seq MOD 2, 'ON DUTY', 'OFF DUTY'),
   ELT(1 + seq MOD 4, 'PATROLING', 'IDLE', 'CHECKPOINT', 'BREAK'),
   NOW() - INTERVAL (seq * 7) MINUTE, NOW() - INTERVAL (seq * 7) MINUTE,
   ${email(userIdx('seq'))}, CONCAT('Petugas Bench ', ${userIdx('seq')})
 FROM seq_1_to_${N.duty};

ANALYZE TABLE tkguest, tkfeed, tkabsen, tkdutyjourney, tkuser, tkusergroup;
SELECT 'tkguest' t, COUNT(*) c FROM tkguest UNION SELECT 'tkfeed', COUNT(*) FROM tkfeed
 UNION SELECT 'tkabsen', COUNT(*) FROM tkabsen UNION SELECT 'tkdutyjourney', COUNT(*) FROM tkdutyjourney
 UNION SELECT 'tkuser', COUNT(*) FROM tkuser;
`;

const p = spawn('mariadb', ['--no-defaults', '-uroot', '--socket=/tmp/kw-bench.sock', 'kawalo_bench'], {
  stdio: ['pipe', 'inherit', 'inherit'],
});
p.stdin.end(sql);
p.on('exit', (code) => process.exit(code ?? 1));

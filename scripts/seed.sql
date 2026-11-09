-- Dummy fixtures for tests; every value is invented.
INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner, Status)
VALUES ('grp-test-1', 'Grup Uji Satu', 'owner@test.id', 'ACTIVE'),
       ('grp-test-2', 'Grup Uji Dua', 'owner@test.id', 'ACTIVE');

INSERT INTO tkuser (UserUID, UserEmail, UserName, Password)
VALUES ('usr-admin', 'admin@test.id', 'Admin Uji', 'rahasia123'),
       ('usr-guard', 'guard@test.id', 'Petugas Uji', 'rahasia123'),
       ('usr-other', 'other@test.id', 'Pengguna Lain', 'rahasia123');

INSERT INTO tkusergroup (Email, GroupUID, AllowAdmin, AllowPatrol, Category, AllowMonPatrol, Status)
VALUES ('admin@test.id', 'grp-test-1', 'YES', 'YES', 'ADMIN', 'YES', 'ACTIVE'),
       ('guard@test.id', 'grp-test-1', 'NO', 'YES', 'GUARD', 'NO', 'ACTIVE'),
       ('guard@test.id', 'grp-test-2', 'NO', 'YES', 'GUARD', 'NO', 'ACTIVE'),
       ('other@test.id', 'grp-test-2', 'NO', 'NO', 'USER', 'NO', 'ACTIVE');

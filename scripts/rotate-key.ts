import { RsaKeyManager } from '@core/security/rsa-key.manager';

async function main() {
  console.log('[CLI] Inisialisasi RsaKeyManager...');
  await RsaKeyManager.init();

  const customKid = process.argv[2];
  console.log(
    `[CLI] Memulai rotasi kunci RSA 4096-bit${customKid ? ` dengan Key ID: ${customKid}` : ''}...`,
  );

  const result = await RsaKeyManager.rotateKey(customKid);
  console.log('====================================================');
  console.log('✅ Rotasi Kunci RSA Berhasil!');
  console.log(`   Kunci Aktif Baru (Active Key ID)  : ${result.activeKid}`);
  console.log(`   Kunci Diarsipkan (Retired Key ID) : ${result.retiredKid}`);
  console.log('====================================================');

  // Bersihkan kunci arsip yang usang (> 30 hari)
  const pruneResult = RsaKeyManager.pruneArchivedKeys(30);
  if (pruneResult.prunedKids.length > 0) {
    console.log(
      `🧹 Kunci Arsip Usang (> 30 hari) yang dibersihkan: ${pruneResult.prunedKids.join(', ')}`,
    );
  }

  console.log('Katalog Kunci Publik di JWKS (/.well-known/jwks.json):');
  const jwks = RsaKeyManager.getJwks();
  for (const k of jwks.keys) {
    console.log(`   • kid: ${k.kid} (alg: ${k.alg}, use: ${k.use})`);
  }
  console.log('====================================================');
}

main().catch((err) => {
  console.error('❌ Gagal melakukan rotasi kunci:', err);
  process.exit(1);
});

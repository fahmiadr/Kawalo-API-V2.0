//! `cargo run --release --bin rotate_key`: rotates the RS256 signing key in `./certs` (port of `scripts/rotate-key.ts` on `legacy/typescript`).

use std::env;

use anyhow::Context;
use kawalo_api::core::security::RsaKeyManager;

/// Archived key files older than this are deleted after a rotation.
const PRUNE_AFTER_DAYS: u64 = 30;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .init();
    run().inspect_err(|err| eprintln!("❌ Gagal melakukan rotasi kunci: {err:#}"))
}

fn run() -> anyhow::Result<()> {
    println!("[CLI] Inisialisasi RsaKeyManager...");
    let certs_dir = env::current_dir()
        .context("reading the working directory")?
        .join("certs");
    let keys = RsaKeyManager::init(certs_dir).context("initialising RSA keys")?;

    let custom_kid = env::args().nth(1);
    match &custom_kid {
        Some(kid) => println!("[CLI] Memulai rotasi kunci RSA 4096-bit dengan Key ID: {kid}..."),
        None => println!("[CLI] Memulai rotasi kunci RSA 4096-bit..."),
    }

    let rotation = keys
        .rotate_key(custom_kid.as_deref())
        .context("rotating the key")?;
    println!("====================================================");
    println!("✅ Rotasi Kunci RSA Berhasil!");
    println!(
        "   Kunci Aktif Baru (Active Key ID)  : {}",
        rotation.active_kid
    );
    println!(
        "   Kunci Diarsipkan (Retired Key ID) : {}",
        rotation.retired_kid
    );
    println!("====================================================");

    let pruned = keys
        .prune_archived_keys(PRUNE_AFTER_DAYS)
        .context("pruning archived keys")?;
    if !pruned.pruned_kids.is_empty() {
        println!(
            "🧹 Kunci Arsip Usang (> {PRUNE_AFTER_DAYS} hari) yang dibersihkan: {}",
            pruned.pruned_kids.join(", ")
        );
    }

    println!("Katalog Kunci Publik di JWKS (/.well-known/jwks.json):");
    for key in keys.jwks().keys {
        println!(
            "   • kid: {} (alg: {}, use: {})",
            key.kid, key.alg, key.key_use
        );
    }
    println!("====================================================");
    Ok(())
}

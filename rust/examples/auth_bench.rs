//! Micro-benchmark of RS256 signing and verification with the key in `./certs` (read, never
//! printed). Run from the API root: `cargo run --release --manifest-path rust/Cargo.toml --example auth_bench`

use std::hint::black_box;
use std::time::Instant;

use kawalo_api::core::security::{
    JwtUserPayload, RsaKeyManager, sign_access_token, verify_access_token,
};

fn report(label: &str, iters: u32, mut f: impl FnMut(u32)) {
    let start = Instant::now();
    for i in 0..iters {
        f(i);
    }
    let per = start.elapsed() / iters;
    println!("{label:<32} {:>9.1} us/op", per.as_secs_f64() * 1e6);
}

fn main() -> anyhow::Result<()> {
    let keys = RsaKeyManager::init(std::env::current_dir()?.join("certs"))?;
    let payload = |n: u32| JwtUserPayload {
        sub: Some(format!("usr-{n}")),
        email: Some("andi@x.id".into()),
        name: Some("Andi".into()),
        role: Some("guard".into()),
        group_uid: Some("g1".into()),
        session_ver: 3,
        ..JwtUserPayload::default()
    };

    let mut tokens = Vec::new();
    report("sign_access_token", 200, |n| {
        let signed = sign_access_token(&keys, 900, &payload(n)).expect("sign");
        tokens.push(signed.token);
    });
    report("verify, first time (RSA)", 200, |n| {
        black_box(verify_access_token(&keys, &tokens[n as usize]));
    });
    report("verify, repeated (cache hit)", 200_000, |n| {
        black_box(verify_access_token(&keys, &tokens[(n % 200) as usize]));
    });
    Ok(())
}

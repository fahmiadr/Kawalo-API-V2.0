//! Tokens minted by Rust verify in the TypeScript `TokenService` and the other way round.
//! Skipped when the TypeScript toolchain is not installed next to `rust/`.

use std::path::{Path, PathBuf};
use std::process::Command;

use kawalo_api::core::security::{
    JwtUserPayload, RsaKeyManager, sign_access_token, verify_access_token,
};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Runs a script from `tests/interop` with the repository's tsx; `None` when tsx is missing.
fn run_ts(script: &str, cwd: &Path, envs: &[(&str, &std::ffi::OsStr)]) -> Option<Value> {
    let root = repo_root();
    let tsx = root.join("node_modules/.bin/tsx");
    if !tsx.exists() {
        eprintln!("skipping: {} not installed", tsx.display());
        return None;
    }
    let output = Command::new(&tsx)
        .current_dir(cwd)
        .arg("--tsconfig")
        .arg(root.join("tsconfig.json"))
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/interop")
                .join(script),
        )
        .envs(envs.iter().copied())
        .env("NODE_ENV", "test")
        .output()
        .expect("run tsx");
    assert!(
        output.status.success(),
        "tsx failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .expect("json line");
    Some(serde_json::from_str(line).expect("json"))
}

#[test]
fn rust_loads_node_written_key_files_and_verifies_their_tokens() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_dir = dir.path().join("certs");
    let Some(result) = run_ts(
        "ts-keys.mts",
        dir.path(),
        &[("KEY_DIR", key_dir.as_os_str())],
    ) else {
        return;
    };

    let keys =
        RsaKeyManager::init_with_modulus(&key_dir, 2048).expect("Rust loads the Node PEM files");
    assert_eq!(keys.active_kid(), "kawalo-key-node");
    let user = verify_access_token(&keys, result["token"].as_str().expect("token"))
        .expect("Rust verifies a token signed by the Node key");
    assert_eq!(
        (user.sub.as_deref(), user.role.as_deref()),
        (Some("node-user"), Some("ADMIN"))
    );
    assert_eq!(user.session_ver, 2);
}

#[test]
fn rust_and_typescript_accept_each_others_tokens() {
    let certs = tempfile::tempdir().expect("tempdir");
    let keys = RsaKeyManager::init_with_modulus(certs.path(), 2048).expect("keys");
    let payload = JwtUserPayload {
        sub: Some("rs-user".into()),
        email: Some("rs@x.id".into()),
        name: Some("RS User".into()),
        role: Some("admin".into()),
        group_uid: Some("g1".into()),
        allow_admin: true,
        session_ver: 3,
        ..JwtUserPayload::default()
    };
    let rust_token = sign_access_token(&keys, 900, &payload).expect("sign").token;

    let Some(result) = run_ts(
        "ts-token.mts",
        certs.path(),
        &[
            ("KEY_DIR", certs.path().as_os_str()),
            ("RUST_TOKEN", rust_token.as_ref()),
        ],
    ) else {
        return;
    };

    let ts_view = &result["verified"];
    assert_eq!(ts_view["sub"], "rs-user", "{result}");
    assert_eq!(ts_view["role"], "ADMIN");
    assert_eq!(ts_view["group_uid"], "g1");
    assert_eq!(ts_view["allow_admin"], true);
    assert_eq!(ts_view["session_ver"], 3);

    let ts_token = result["token"].as_str().expect("ts token");
    let rust_view =
        verify_access_token(&keys, ts_token).expect("rust verifies the TypeScript token");
    assert_eq!(rust_view.sub.as_deref(), Some("ts-user"));
    assert_eq!(rust_view.role.as_deref(), Some("GUARD"));
    assert_eq!(rust_view.group_uid.as_deref(), Some("g9"));
    assert!(rust_view.allow_patrol && !rust_view.allow_admin);
    assert_eq!(rust_view.session_ver, 7);
}

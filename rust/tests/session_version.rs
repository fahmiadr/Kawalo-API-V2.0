//! Session version semantics against the scratch Redis.

mod common;

use common::TestApp;
use kawalo_api::core::security::SessionVersions;
use redis::AsyncCommands;
use uuid::Uuid;

fn user() -> String {
    format!("sv-{}", Uuid::new_v4())
}

#[tokio::test]
async fn get_or_init_creates_version_one_with_a_ttl() {
    let app = TestApp::start().await;
    let id = user();
    assert_eq!(
        app.state
            .sessions
            .get_or_init_version(&id)
            .await
            .expect("init"),
        1
    );
    assert_eq!(
        app.state
            .sessions
            .get_or_init_version(&id)
            .await
            .expect("again"),
        1
    );

    let mut redis = app.state.redis.clone();
    let key = SessionVersions::key(&id);
    assert_eq!(key, format!("auth:user:{id}:session_ver"));
    let ttl: i64 = redis.ttl(&key).await.expect("ttl");
    assert!((1_209_000..=1_209_600).contains(&ttl), "ttl {ttl}");
}

#[tokio::test]
async fn increment_is_atomic_and_revokes_older_tokens() {
    let app = TestApp::start().await;
    let (id, sessions) = (user(), &app.state.sessions);
    assert_eq!(sessions.get_or_init_version(&id).await.expect("init"), 1);
    assert_eq!(sessions.increment_version(&id).await.expect("inc"), 2);
    assert_eq!(sessions.increment_version(&id).await.expect("inc"), 3);
    assert!(!sessions.validate_or_rehydrate(&id, 2).await.expect("stale"));
    assert!(
        sessions
            .validate_or_rehydrate(&id, 3)
            .await
            .expect("current")
    );
    assert!(sessions.validate_or_rehydrate(&id, 9).await.expect("newer"));
}

#[tokio::test]
async fn missing_key_is_rehydrated_from_the_token_version() {
    let app = TestApp::start().await;
    let (id, sessions) = (user(), &app.state.sessions);
    assert!(
        sessions
            .validate_or_rehydrate(&id, 5)
            .await
            .expect("rehydrate")
    );
    let mut redis = app.state.redis.clone();
    let stored: String = redis.get(SessionVersions::key(&id)).await.expect("stored");
    assert_eq!(stored, "5");
    assert!(
        !sessions
            .validate_or_rehydrate(&id, 4)
            .await
            .expect("older than rehydrated")
    );
}

#[tokio::test]
async fn invalid_inputs_are_rejected() {
    let app = TestApp::start().await;
    let sessions = &app.state.sessions;
    assert!(
        !sessions
            .validate_or_rehydrate("", 1)
            .await
            .expect("empty user")
    );
    assert!(
        !sessions
            .validate_or_rehydrate(&user(), 0)
            .await
            .expect("zero version")
    );
    assert!(sessions.get_or_init_version("").await.is_err());
    assert!(sessions.increment_version("").await.is_err());
}

#[tokio::test]
async fn corrupt_values_never_validate() {
    let app = TestApp::start().await;
    let (id, sessions) = (user(), &app.state.sessions);
    let mut redis = app.state.redis.clone();
    let _: () = redis
        .set(SessionVersions::key(&id), "abc")
        .await
        .expect("set");
    assert!(!sessions.validate_or_rehydrate(&id, 1).await.expect("nan"));
    assert!(sessions.get_or_init_version(&id).await.is_err());
}

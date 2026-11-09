//! Separate binary: killing pub/sub connections server-side would disturb PTT tests running in
//! parallel on the same Redis.

mod common;
mod ptt_support;

use std::time::Duration;

use common::TestApp;
use ptt_support::*;

async fn numsub(app: &TestApp, channel: &str) -> i64 {
    let mut redis = app.state.redis.clone();
    let (_, count): (String, i64) = redis::cmd("PUBSUB")
        .arg("NUMSUB")
        .arg(channel)
        .query_async(&mut redis)
        .await
        .expect("numsub");
    count
}

async fn eventually_numsub(app: &TestApp, channel: &str, want: i64) {
    for _ in 0..80 {
        if numsub(app, channel).await == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "{channel} never reached {want} subscribers, has {}",
        numsub(app, channel).await
    );
}

#[tokio::test]
async fn the_subscriber_recovers_after_redis_drops_its_connection() {
    let app = TestApp::start().await;
    let a = app.serve().await;
    let b = second_instance(&app).await;
    let squad = Squad::new();
    let (t1, t2) = (
        squad.token(&app, "u1", false).await,
        squad.token(&app, "u2", false).await,
    );
    let mut ws_a = join(a, &squad.group, &t1).await;
    ws_a.json().await;
    let mut ws_b = join(b, &squad.group, &t2).await;
    ws_b.json().await;
    ws_a.json().await;

    // Kill every pub/sub client on the server (including the test's own: it uses none).
    let mut redis = app.state.redis.clone();
    let _: i64 = redis::cmd("CLIENT")
        .arg("KILL")
        .arg("TYPE")
        .arg("pubsub")
        .query_async(&mut redis)
        .await
        .expect("kill");

    let ctl = format!("ptt:room:{}:ctl", squad.group);
    eventually_numsub(&app, &ctl, 2).await;
    let token3 = squad.token(&app, "u3", false).await;
    let mut third = join(b, &squad.group, &token3).await;
    third.json().await;
    assert_eq!(
        ws_a.json().await,
        count(3),
        "relay works again after the reconnect"
    );
}

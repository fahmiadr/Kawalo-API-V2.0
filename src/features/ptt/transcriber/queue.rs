//! The transcription queue is `tkpttrecording` itself; there is no job table.
//!
//! Every time comparison uses the database clock (`NOW()`), never the worker's, so grace and
//! staleness agree with `CreatedAt` whatever the worker host's clock says.
//!
//! A claim is the pair `(RecordingUID, TranscriptAttempts)`: every claim increments the counter,
//! so once a stale claim has been recovered and taken by another worker, the first worker's late
//! release or failure matches no row.
//!
//! `TranscriptClaimedAt` is the time of the last server claim and stays set after the job ends
//! (only a release clears it). A row the server failed on is retried no sooner than one grace
//! period after that claim, so a failing recording does not hog the worker ahead of newer turns.

use sqlx::MySqlPool;

/// Shared `WHERE` clause; binds: max attempts, grace seconds (twice), group, group.
macro_rules! eligible {
    () => {
        "DeletedAt IS NULL AND Transcript IS NULL AND TranscriptAttempts < ? \
         AND CASE \
             WHEN TranscriptStatus IN ('unsupported', 'failed') THEN \
                 TranscriptClaimedAt IS NULL OR TranscriptClaimedAt < NOW() - INTERVAL ? SECOND \
             WHEN TranscriptStatus IS NULL THEN CreatedAt < NOW() - INTERVAL ? SECOND \
             ELSE FALSE END \
         AND (? IS NULL OR GroupUID = ?)"
    };
}

/// Lost claim races before giving up until the next poll; each retry reads a fresh candidate.
const CLAIM_RETRIES: usize = 5;

#[derive(Debug, Clone, Copy)]
pub struct Scope<'a> {
    pub max_attempts: u8,
    pub grace_secs: u64,
    pub stale_secs: u64,
    pub only_group: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct Claim {
    pub id: String,
    pub file_path: String,
    /// `TranscriptAttempts` after this claim, starting at 1.
    pub attempt: u8,
}

/// Claims the oldest eligible recording.
///
/// Portable to MariaDB 10.5 (no `SKIP LOCKED`, which arrived in 10.6) and MySQL (no `RETURNING`):
/// read a candidate without locking, then claim it with an `UPDATE` that repeats the eligibility
/// test and pins the attempt counter. InnoDB evaluates that `WHERE` on the latest row version
/// under the row lock, so of two workers racing for one row exactly one sees `rows_affected = 1`.
pub async fn claim(db: &MySqlPool, scope: Scope<'_>) -> sqlx::Result<Option<Claim>> {
    for _ in 0..CLAIM_RETRIES {
        let candidate: Option<(String, String, u8)> = sqlx::query_as(concat!(
            "SELECT RecordingUID, FilePath, TranscriptAttempts FROM tkpttrecording WHERE ",
            eligible!(),
            " ORDER BY StartedAt, RecordingUID LIMIT 1"
        ))
        .bind(scope.max_attempts)
        .bind(scope.grace_secs)
        .bind(scope.grace_secs)
        .bind(scope.only_group)
        .bind(scope.only_group)
        .fetch_optional(db)
        .await?;
        let Some((id, file_path, attempts)) = candidate else {
            return Ok(None);
        };
        let won = sqlx::query(concat!(
            "UPDATE tkpttrecording SET TranscriptStatus = 'processing', \
             TranscriptClaimedAt = NOW(), TranscriptAttempts = TranscriptAttempts + 1 \
             WHERE RecordingUID = ? AND TranscriptAttempts = ? AND ",
            eligible!()
        ))
        .bind(&id)
        .bind(attempts)
        .bind(scope.max_attempts)
        .bind(scope.grace_secs)
        .bind(scope.grace_secs)
        .bind(scope.only_group)
        .bind(scope.only_group)
        .execute(db)
        .await?
        .rows_affected()
            == 1;
        if won {
            return Ok(Some(Claim {
                id,
                file_path,
                attempt: attempts.saturating_add(1),
            }));
        }
    }
    Ok(None)
}

/// Returns claims older than `stale_secs` to the queue as `failed`, or ends them as `error` when
/// no attempt is left. Returns the number of rows changed.
pub async fn recover_stale(db: &MySqlPool, scope: Scope<'_>) -> sqlx::Result<u64> {
    Ok(sqlx::query(
        "UPDATE tkpttrecording SET TranscriptStatus = \
         CASE WHEN TranscriptAttempts < ? THEN 'failed' ELSE 'error' END \
         WHERE TranscriptStatus = 'processing' \
         AND TranscriptClaimedAt < NOW() - INTERVAL ? SECOND \
         AND (? IS NULL OR GroupUID = ?)",
    )
    .bind(scope.max_attempts)
    .bind(scope.stale_secs)
    .bind(scope.only_group)
    .bind(scope.only_group)
    .execute(db)
    .await?
    .rows_affected())
}

/// Stores the server transcript unless a device text (or a delete) got there first. Not tied to
/// the claim: a correct text is worth keeping even if the claim went stale meanwhile.
pub async fn store_text(
    db: &MySqlPool,
    claim: &Claim,
    text: &str,
    locale: &str,
) -> sqlx::Result<bool> {
    Ok(sqlx::query(
        "UPDATE tkpttrecording SET Transcript = ?, TranscriptLocale = ?, TranscriptAt = NOW(), \
         TranscriptSource = 'server', TranscriptStatus = 'done' \
         WHERE RecordingUID = ? AND Transcript IS NULL AND DeletedAt IS NULL",
    )
    .bind(text)
    .bind(locale)
    .bind(&claim.id)
    .execute(db)
    .await?
    .rows_affected()
        == 1)
}

/// Ends the claim with `status` (`failed`, `no_speech` or `error`).
pub async fn settle(db: &MySqlPool, claim: &Claim, status: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tkpttrecording SET TranscriptStatus = ? \
         WHERE RecordingUID = ? AND TranscriptStatus = 'processing' AND TranscriptAttempts = ?",
    )
    .bind(status)
    .bind(&claim.id)
    .bind(claim.attempt)
    .execute(db)
    .await?;
    Ok(())
}

/// Gives the claim back without counting it as an attempt (shutdown, missing binary). `failed`
/// is the re-queue status: the row was eligible when claimed, and with the claim time cleared it
/// is eligible again at once.
pub async fn release(db: &MySqlPool, claim: &Claim) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tkpttrecording SET TranscriptStatus = 'failed', TranscriptClaimedAt = NULL, \
         TranscriptAttempts = TranscriptAttempts - 1 \
         WHERE RecordingUID = ? AND TranscriptStatus = 'processing' AND TranscriptAttempts = ?",
    )
    .bind(&claim.id)
    .bind(claim.attempt)
    .execute(db)
    .await?;
    Ok(())
}

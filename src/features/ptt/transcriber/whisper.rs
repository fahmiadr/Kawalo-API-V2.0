//! Runs `whisper-cli` (whisper.cpp) on one WAV file and cleans its output.

use std::io;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

/// `-nt` output of a 30 s turn is a few hundred bytes; anything past this is dropped unread.
const MAX_STDOUT: usize = 64 * 1024;

/// Whisper marks non-speech as `[BLANK_AUDIO]`, `[Musik]`, `(tertawa)` or `*suara*`.
static TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]*\]|\([^)]*\)|\*[^*]*\*").expect("static regex is valid"));

#[derive(Debug)]
pub struct Invocation<'a> {
    pub bin: &'a Path,
    pub model: &'a Path,
    pub vad_model: Option<&'a Path>,
    pub wav: &'a Path,
    pub language: &'a str,
    pub threads: u32,
    pub timeout: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum WhisperError {
    /// The binary is missing or not executable: a deployment problem, not a property of the
    /// recording, so the caller must not count it as an attempt.
    #[error("could not start whisper-cli: {0}")]
    Spawn(io::Error),
    #[error("whisper-cli timed out after {0:?} and was killed")]
    Timeout(Duration),
    #[error("whisper-cli exited with {0}")]
    Exit(ExitStatus),
    #[error("reading whisper-cli failed: {0}")]
    Io(io::Error),
    #[error("whisper-cli output is not UTF-8")]
    NotUtf8,
}

/// Returns the raw stdout. Arguments are passed as separate strings, never through a shell. The
/// child inherits stderr (whisper-cli prints only errors with `-np`), so failures reach the
/// journal.
pub async fn run(inv: &Invocation<'_>) -> Result<String, WhisperError> {
    let mut cmd = Command::new(inv.bin);
    cmd.arg("-m")
        .arg(inv.model)
        .arg("-f")
        .arg(inv.wav)
        .arg("-l")
        .arg(inv.language)
        .arg("-t")
        .arg(inv.threads.to_string())
        .args(["-nt", "-np"]);
    if let Some(vad) = inv.vad_model {
        cmd.arg("--vad").arg("-vm").arg(vad);
    }
    // ponytail: only the direct child is killed on timeout; whisper-cli spawns no helpers. Use a
    // process group (and libc kill(-pgid)) if STT_BIN_PATH ever points at a wrapper script.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(WhisperError::Spawn)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| WhisperError::Io(io::Error::other("stdout not captured")))?;

    let finished = tokio::time::timeout(inv.timeout, async {
        let (out, status) = tokio::join!(read_bounded(&mut stdout), child.wait());
        (out, status)
    })
    .await;
    let (out, status) = match finished {
        Ok(done) => done,
        Err(_) => {
            // `kill` also waits, so the child is reaped here and not left as a zombie.
            let _ = child.kill().await;
            return Err(WhisperError::Timeout(inv.timeout));
        }
    };
    let status = status.map_err(WhisperError::Io)?;
    if !status.success() {
        return Err(WhisperError::Exit(status));
    }
    String::from_utf8(out.map_err(WhisperError::Io)?).map_err(|_| WhisperError::NotUtf8)
}

/// Keeps the first [`MAX_STDOUT`] bytes and drains the rest, so a chatty child never blocks on a
/// full pipe.
async fn read_bounded(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            return Ok(out);
        }
        let room = MAX_STDOUT.saturating_sub(out.len());
        out.extend_from_slice(&buf[..n.min(room)]);
    }
}

/// Strips non-speech tags, collapses whitespace and caps the text at `max_chars` characters.
pub fn clean(raw: &str, max_chars: usize) -> String {
    let stripped = TAGS.replace_all(raw, " ");
    let words = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    match words.char_indices().nth(max_chars) {
        Some((cut, _)) => words[..cut].trim_end().to_owned(),
        None => words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_whitespace_are_removed() {
        assert_eq!(clean(" [BLANK_AUDIO]\n", 2000), "");
        assert_eq!(clean("(musik) *suara angin*", 2000), "");
        assert_eq!(
            clean("  Pos dua  aman,\n [Musik] ganti.\n", 2000),
            "Pos dua aman, ganti."
        );
    }

    #[test]
    fn text_is_capped_by_characters_not_bytes() {
        let long = "é".repeat(2100);
        let cut = clean(&long, 2000);
        assert_eq!(cut.chars().count(), 2000);
        assert_eq!(clean("satu dua", 4), "satu");
    }
}

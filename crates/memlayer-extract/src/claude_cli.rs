// Generated with AI Coding Rules Hub
//! Shell-out client for the local `claude` CLI.
//!
//! Mirrors the proxy-strip + model selection pattern proven in
//! `memlayer-eval/src/judge.rs:40-65`. Capital One's enterprise environment
//! sets `ALL_PROXY=socks5h://...` for awsproxy; the Bedrock SDK behind
//! `claude` rejects socks5h URIs, so we drop those vars before spawning.
//!
//! For tests, `MockClaudeClient` lets us script responses without shelling
//! out (TS-6 / TS-7 in P2 use this).

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::debug;

/// Timeout for a single Claude CLI call (Bedrock proxy can be slow).
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(120);

pub const HAIKU_MODEL: &str = "claude-haiku-4-5";
pub const SONNET_MODEL: &str = "claude-sonnet-4-6";

/// Abstract Claude client. Real impl shells out; mock returns canned output.
#[async_trait]
pub trait ClaudeClient: Send + Sync {
    /// Send a single prompt to the named model, return the raw text response
    /// (trimmed of trailing whitespace). Errors are wrapped with anyhow context.
    ///
    /// `model` may be a CLI id (`claude-haiku-4-5`), an alias (`haiku` /
    /// `sonnet`), or empty (use the agent's default). The production client
    /// retries other known ids and the CLI default when the requested model
    /// is not installed.
    async fn ask(&self, prompt: &str, model: &str) -> Result<String>;
}

/// Production client: shells out to the `claude` CLI on PATH.
pub struct ClaudeCliClient;

impl ClaudeCliClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ClaudeCliClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClaudeClient for ClaudeCliClient {
    async fn ask(&self, prompt: &str, model: &str) -> Result<String> {
        let candidates = claude_model_candidates(model);
        let mut last_err: Option<anyhow::Error> = None;
        for cand in &candidates {
            match invoke_claude(prompt, cand).await {
                Ok(text) => return Ok(text),
                Err(e) => {
                    let msg = format!("{e:#}");
                    if looks_like_unavailable_model(&msg) {
                        tracing::info!(
                            requested = %model,
                            tried = %if cand.is_empty() { "<agent-default>" } else { cand.as_str() },
                            "model unavailable; trying next candidate"
                        );
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no Claude model available")))
    }
}

/// Preferred model, then the other shipped id, then the agent CLI default
/// (omit `--model`). `MEMLAYER_CLAUDE_MODEL` wins when set.
pub fn claude_model_candidates(preferred: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |out: &mut Vec<String>, s: &str| {
        let t = s.trim();
        if out.iter().any(|x| x == t) {
            return;
        }
        out.push(t.to_string());
    };

    if let Ok(env) = std::env::var("MEMLAYER_CLAUDE_MODEL") {
        if !env.trim().is_empty() {
            push(&mut out, &env);
        }
    }

    let pref = preferred.trim();
    if !pref.is_empty() {
        push(&mut out, pref);
        match pref.to_ascii_lowercase().as_str() {
            "haiku" => push(&mut out, HAIKU_MODEL),
            "sonnet" => push(&mut out, SONNET_MODEL),
            "claude-4.5-haiku" => push(&mut out, HAIKU_MODEL),
            "claude-4.6-sonnet" => push(&mut out, SONNET_MODEL),
            _ => {}
        }
    }

    push(&mut out, SONNET_MODEL);
    push(&mut out, HAIKU_MODEL);
    push(&mut out, ""); // agent / `claude` default — no --model flag
    out
}

pub fn looks_like_unavailable_model(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("unknown model")
        || lower.contains("invalid model")
        || lower.contains("model not found")
        || lower.contains("not available")
        || lower.contains("does not exist")
        || lower.contains("bad model")
        || lower.contains("unrecognized model")
        || lower.contains("empty response for model")
        || lower.contains("model_not_found")
}

async fn invoke_claude(prompt: &str, model: &str) -> Result<String> {
    let mut cmd = Command::new("claude");
    cmd.arg("-p").arg(prompt);
    if !model.is_empty() {
        cmd.arg("--model").arg(model);
    }
    cmd.arg("--output-format").arg("text");
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Same proxy strip as memlayer-eval/src/judge.rs:46-51 — Bedrock SDK
        // doesn't speak socks5h, which Capital One's awsproxy sets.
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .env_remove("FTP_PROXY")
        .env_remove("ftp_proxy")
        .env_remove("GRPC_PROXY")
        .env_remove("grpc_proxy")
        .spawn()
        .context("spawn claude CLI — is `claude` on PATH?")?;

    let output = timeout(CLAUDE_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("claude CLI timed out after {}s", CLAUDE_TIMEOUT.as_secs()))?
        .context("wait for claude CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "claude CLI exited with {}: stderr={} stdout={}",
            output.status,
            stderr,
            stdout
        );
    }

    let text = String::from_utf8(output.stdout)
        .context("claude CLI output was not valid UTF-8")?
        .trim()
        .to_string();
    if text.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "claude CLI returned empty response for model '{}' (bad model ID?): stderr={}",
            if model.is_empty() { "<agent-default>" } else { model },
            stderr
        );
    }
    debug!(model = %if model.is_empty() { "<agent-default>" } else { model }, response_len = text.len(), "claude CLI ok");
    Ok(text)
}

/// Test double: deque of canned responses, popped in FIFO order. Construct
/// with [`MockClaudeClient::with_responses`]; tests can also count calls via
/// the `Arc<Mutex<...>>` interior. Signal exhaustion by popping a `None`,
/// at which point `ask` returns an error rather than panicking.
pub struct MockClaudeClient {
    responses: Arc<parking_lot::Mutex<std::collections::VecDeque<String>>>,
    call_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl MockClaudeClient {
    pub fn with_responses<I, S>(responses: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let q: std::collections::VecDeque<String> =
            responses.into_iter().map(|s| s.into()).collect();
        Self {
            responses: Arc::new(parking_lot::Mutex::new(q)),
            call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl ClaudeClient for MockClaudeClient {
    async fn ask(&self, _prompt: &str, _model: &str) -> Result<String> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut q = self.responses.lock();
        match q.pop_front() {
            Some(s) => Ok(s),
            None => bail!("MockClaudeClient: response queue exhausted"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_expand_alias_and_end_with_agent_default() {
        let c = claude_model_candidates("haiku");
        assert_eq!(c[0], "haiku");
        assert!(c.iter().any(|m| m == HAIKU_MODEL));
        assert!(c.iter().any(|m| m == SONNET_MODEL));
        assert_eq!(c.last().map(String::as_str), Some(""));
    }

    #[test]
    fn candidates_env_override_first() {
        std::env::set_var("MEMLAYER_CLAUDE_MODEL", "claude-opus-4-6");
        let c = claude_model_candidates("haiku");
        std::env::remove_var("MEMLAYER_CLAUDE_MODEL");
        assert_eq!(c[0], "claude-opus-4-6");
    }

    #[test]
    fn unavailable_model_errors_are_detected() {
        assert!(looks_like_unavailable_model("unknown model 'claude-haiku-4-5'"));
        assert!(looks_like_unavailable_model("empty response for model 'x'"));
        assert!(!looks_like_unavailable_model("spawn claude CLI — is `claude` on PATH?"));
        assert!(!looks_like_unavailable_model("claude CLI timed out after 120s"));
    }
}


//! Outbound email (Phase 5, REQ-EMAIL-001..004).
//!
//! Design: a [`EmailSender`] trait is the seam; the production impl talks SMTP
//! via `lettre` ([`SmtpSender`]), tests use [`InMemorySender`]. Templates are
//! plain `format!` HTML — no template engine dependency (see AD-9: the project
//! already favours zero-runtime-dependency string building for its own HTML).
//!
//! Tokens are the caller's business (see [`crate::store`]): this module only
//! renders and sends. Plaintext tokens must never be persisted — the store
//! keeps sha256 hashes.

use async_trait::async_trait;
use lettre::message::Message;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use serde::{Deserialize, Serialize};

/// An email ready to be sent. Kept minimal so the trait stays easy to mock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body_html: String,
}

/// The seam between MoriNet and the outside world.
#[async_trait]
pub trait EmailSender: Send + Sync {
    /// Deliver one email. Errors propagate so callers can decide what to do
    /// (fail the request, warn, retry) rather than swallowing silently.
    async fn send(&self, email: &Email) -> anyhow::Result<()>;
}

// ---------- SMTP config (REQ-EMAIL-001) ----------

/// `[smtp]` section of config.toml. All fields optional — a missing/empty
/// section disables delivery and the app falls back to the admin-visible flow.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SmtpConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub from: Option<String>,
    /// `true` (default) = STARTTLS on port 587, `false` = implicit TLS (465).
    #[serde(default)]
    pub starttls: Option<bool>,
}

impl SmtpConfig {
    /// Is the section complete enough to attempt delivery? Requires host+from;
    /// credentials are only needed when `user` is set (open relay case).
    pub fn is_usable(&self) -> bool {
        self.host.as_ref().is_some_and(|h| !h.trim().is_empty())
            && self.from.as_ref().is_some_and(|f| !f.trim().is_empty())
    }
}

// ---------- production sender ----------

/// Real SMTP sender backed by `lettre` (AD-7).
pub struct SmtpSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl SmtpSender {
    /// Build a sender from config. Errors when the section is incomplete or
    /// the port is out of range, so misconfiguration fails loudly at boot.
    pub fn from_config(cfg: &SmtpConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(cfg.is_usable(), "smtp section needs both 'host' and 'from'");
        let host = cfg.host.clone().unwrap_or_default();
        let from = cfg.from.clone().unwrap_or_default();
        let use_starttls = cfg.starttls.unwrap_or(true);
        let port = match cfg.port {
            Some(p) => p,
            None if use_starttls => 587,
            None => 465,
        };

        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
            .map_err(|e| anyhow::anyhow!("smtp: cannot use STARTTLS with '{host}': {e}"))?
            .port(port);
        if !use_starttls {
            builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
                .map_err(|e| anyhow::anyhow!("smtp: cannot use implicit TLS with '{host}': {e}"))?
                .port(port);
        }
        if let Some(user) = &cfg.user {
            if !user.trim().is_empty() {
                builder = builder.credentials(Credentials::new(
                    user.clone(),
                    cfg.pass.clone().unwrap_or_default(),
                ));
            }
        }
        Ok(Self {
            transport: builder.build(),
            from,
        })
    }
}

#[async_trait]
impl EmailSender for SmtpSender {
    async fn send(&self, email: &Email) -> anyhow::Result<()> {
        let from: lettre::message::Mailbox = format!("<{}>", self.from).parse()?;
        let to: lettre::message::Mailbox = email.to.parse()?;
        // Send as a single-part text/html message: the body IS the HTML, and
        // `SinglePart::html` stamps the content-type on the part (a top-level
        // `.body(String)` would default to text/plain and clients would render
        // raw tags).
        let part = lettre::message::SinglePart::html(email.body_html.clone());
        let msg = Message::builder()
            .from(from)
            .to(to)
            .subject(&email.subject)
            .singlepart(part)
            .map_err(|e| anyhow::anyhow!("smtp: build failed: {e}"))?;
        self.transport.send(msg).await?;
        Ok(())
    }
}

// ---------- test sender ----------

/// In-memory sender: records every email, never touches the network.
/// This is the `InMemoryMock` the roadmap asks for (REQ-EMAIL-002).
#[derive(Default)]
pub struct InMemorySender {
    sent: std::sync::Mutex<Vec<Email>>,
}

impl InMemorySender {
    pub fn new() -> Self {
        Self::default()
    }

    /// All emails sent so far, in order.
    pub fn sent(&self) -> Vec<Email> {
        self.sent.lock().expect("in-memory sender mutex").clone()
    }

    pub fn count(&self) -> usize {
        self.sent.lock().expect("in-memory sender mutex").len()
    }

    /// The single email sent; panics if zero or many (test convenience).
    pub fn only(&self) -> Email {
        let all = self.sent();
        assert_eq!(
            all.len(),
            1,
            "expected exactly one email, got {}",
            all.len()
        );
        all.into_iter().next().unwrap()
    }
}

#[async_trait]
impl EmailSender for InMemorySender {
    async fn send(&self, email: &Email) -> anyhow::Result<()> {
        self.sent
            .lock()
            .expect("in-memory sender mutex")
            .push(email.clone());
        Ok(())
    }
}

// ---------- templates (REQ-EMAIL-003, REQ-EMAIL-004) ----------

fn layout(title: &str, inner: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>{title}</title></head>
<body style="font-family:system-ui,sans-serif;max-width:560px;margin:40px auto;color:#222">
<h1 style="font-size:20px">MoriNet</h1>
{inner}
<hr style="margin:32px 0;border:none;border-top:1px solid #eee">
<p style="color:#888;font-size:12px">You receive this because someone set up a MoriNet server.</p>
</body></html>"#
    )
}

/// Invitation email (REQ-EMAIL-003). `invite_url` is the absolute link with the
/// plaintext token — the only place the plaintext ever appears.
pub fn invitation_body(invite_url: &str, role: &str) -> String {
    layout(
        "You're invited to MoriNet",
        &format!(
            r#"<p>You have been invited to a MoriNet server as a <strong>{role}</strong>.</p>
<p><a href="{url}" style="display:inline-block;padding:12px 20px;background:#2b6cb0;color:#fff;text-decoration:none;border-radius:6px">Accept the invitation</a></p>
<p style="color:#666;font-size:13px">Or paste this link into your browser:<br><span style="word-break:break-all">{url}</span></p>
<p style="color:#666;font-size:13px">This link is single-use and expires in 24 hours. If you weren't expecting it, you can ignore this email.</p>"#,
            url = invite_url
        ),
    )
}

/// Password-reset email (REQ-EMAIL-004).
pub fn reset_body(reset_url: &str) -> String {
    layout(
        "Reset your MoriNet password",
        &format!(
            r#"<p>Someone (hopefully you) requested a password reset for your MoriNet account.</p>
<p><a href="{url}" style="display:inline-block;padding:12px 20px;background:#2b6cb0;color:#fff;text-decoration:none;border-radius:6px">Choose a new password</a></p>
<p style="color:#666;font-size:13px">Or paste this link into your browser:<br><span style="word-break:break-all">{url}</span></p>
<p style="color:#666;font-size:13px">This link is single-use and expires in 1 hour. If you weren't expecting it, your password is unchanged and you can ignore this email.</p>"#,
            url = reset_url
        ),
    )
}

/// Build the full invitation email (subject + rendered body).
pub fn invitation_email(to: &str, invite_url: &str, role: &str) -> Email {
    Email {
        to: to.to_string(),
        subject: "You've been invited to MoriNet".to_string(),
        body_html: invitation_body(invite_url, role),
    }
}

/// Build the full password-reset email.
pub fn reset_email(to: &str, reset_url: &str) -> Email {
    Email {
        to: to.to_string(),
        subject: "Reset your MoriNet password".to_string(),
        body_html: reset_body(reset_url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtp_config_usable_requires_host_and_from() {
        assert!(!SmtpConfig::default().is_usable(), "empty section unusable");
        assert!(!SmtpConfig {
            host: Some("smtp.x".into()),
            ..Default::default()
        }
        .is_usable());
        assert!(SmtpConfig {
            host: Some("smtp.x".into()),
            from: Some("me@x".into()),
            ..Default::default()
        }
        .is_usable());
        assert!(
            !SmtpConfig {
                host: Some("  ".into()),
                from: Some("me@x".into()),
                ..Default::default()
            }
            .is_usable(),
            "blank host is not usable"
        );
    }

    #[test]
    fn smtp_sender_from_config_rejects_incomplete_section() {
        let err = SmtpSender::from_config(&SmtpConfig::default())
            .err()
            .expect("empty config must fail");
        assert!(err.to_string().contains("smtp"), "got: {err}");
    }

    #[tokio::test]
    async fn in_memory_sender_records_sent_email() {
        let s = InMemorySender::new();
        assert_eq!(s.count(), 0);
        let email = invitation_email("a@x.fr", "https://x/invite/tok", "reader");
        s.send(&email).await.unwrap();
        assert_eq!(s.count(), 1);
        assert_eq!(s.only().to, "a@x.fr");
    }

    #[test]
    fn invitation_template_embeds_link_and_role() {
        let body = invitation_body("https://mori/invite/abc123", "reader");
        assert!(
            body.contains("https://mori/invite/abc123"),
            "link must be present"
        );
        assert!(body.contains("reader"), "role must be present");
        assert!(
            body.starts_with("<!DOCTYPE html>"),
            "must be a full HTML doc"
        );
    }

    #[test]
    fn reset_template_embeds_link() {
        let body = reset_body("https://mori/reset/tok");
        assert!(body.contains("https://mori/reset/tok"));
        assert!(body.contains("expires in 1 hour"), "reset TTL stated as 1h");
    }

    #[test]
    fn invitation_states_24h_expiry_and_single_use() {
        let body = invitation_body("https://mori/invite/t", "reader");
        assert!(body.contains("single-use"), "single-use must be stated");
        assert!(body.contains("24 hours"), "invitation TTL stated as 24h");
    }

    #[test]
    fn email_builders_set_subject_and_recipient() {
        let inv = invitation_email("b@x.fr", "https://x/i", "reader");
        assert_eq!(inv.subject, "You've been invited to MoriNet");
        assert_eq!(inv.to, "b@x.fr");
        let rst = reset_email("c@x.fr", "https://x/r");
        assert_eq!(rst.subject, "Reset your MoriNet password");
        assert_eq!(rst.to, "c@x.fr");
    }
}

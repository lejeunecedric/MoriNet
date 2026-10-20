//! HMAC-signed session cookie codec (AD-4: HttpOnly signed cookies, no JWT).
//!
//! Token format: `b64url(email).expiry_unix.b64url(hmac_sha256(secret, "email.expiry"))`

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<sha2::Sha256>;

/// Default session lifetime: 24 hours.
pub const DEFAULT_TTL_SECS: u64 = 24 * 3600;

pub struct SessionCodec {
    secret: Vec<u8>,
}

pub struct Claims {
    pub email: String,
    pub expires_at: u64,
}

fn b64(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

impl SessionCodec {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self { secret: secret.into() }
    }

    fn mac(&self, payload: &str) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("hmac accepts any key length");
        mac.update(payload.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    /// Sign a session token for `email`, valid from `now` for `ttl_secs`.
    pub fn sign(&self, email: &str, now: u64, ttl_secs: u64) -> String {
        let expires_at = now.saturating_add(ttl_secs);
        let payload = format!("{}.{}", b64(email.as_bytes()), expires_at);
        let sig = b64(&self.mac(&payload));
        format!("{}.{}", payload, sig)
    }

    /// Verify a token against `now`. Returns the claims on success, None on any
    /// tampering, bad encoding, expired token, or wrong signature.
    pub fn verify(&self, token: &str, now: u64) -> Option<Claims> {
        let mut parts = token.split('.');
        let email_b64 = parts.next()?;
        let expiry = parts.next()?;
        let sig_b64 = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if email_b64.is_empty() || expiry.is_empty() || sig_b64.is_empty() {
            return None;
        }

        // Constant-time signature comparison.
        let payload = format!("{}.{}", email_b64, expiry);
        let expected = self.mac(&payload);
        let got = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;
        if got.len() != expected.len() {
            return None;
        }
        if bool::from(got.as_slice().ct_eq(&expected)) {
            // Signature valid; continue verifying the claims.
        } else {
            return None;
        }

        let expires_at: u64 = expiry.parse().ok()?;
        if now >= expires_at {
            return None;
        }
        let email = URL_SAFE_NO_PAD.decode(email_b64).ok()?;
        let email = String::from_utf8(email).ok()?;
        Some(Claims { email, expires_at })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-0123456789abcdef";
    const NOW: u64 = 1_700_000_000;

    #[test]
    fn sign_then_verify_roundtrip() {
        let codec = SessionCodec::new(SECRET);
        let token = codec.sign("admin@treklair.fr", NOW, DEFAULT_TTL_SECS);
        let claims = codec.verify(&token, NOW).expect("valid token verifies");
        assert_eq!(claims.email, "admin@treklair.fr");
        assert_eq!(claims.expires_at, NOW + DEFAULT_TTL_SECS);
    }

    #[test]
    fn tampered_email_is_rejected() {
        let codec = SessionCodec::new(SECRET);
        let token = codec.sign("admin@treklair.fr", NOW, 60);
        // Flip the email part to another user; signature no longer matches.
        let evil = token.replacen(&b64(b"admin@treklair.fr"), &b64(b"evil@evil.fr"), 1);
        assert!(codec.verify(&evil, NOW).is_none(), "tampered token rejected");
    }

    #[test]
    fn tampered_expiry_is_rejected() {
        let codec = SessionCodec::new(SECRET);
        let token = codec.sign("admin@treklair.fr", NOW, 60);
        let mut parts: Vec<String> = token.split('.').map(String::from).collect();
        parts[1] = (NOW + 999_999).to_string();
        let evil = parts.join(".");
        assert!(codec.verify(&evil, NOW).is_none(), "extended expiry rejected");
    }

    #[test]
    fn expired_token_is_rejected() {
        let codec = SessionCodec::new(SECRET);
        let token = codec.sign("admin@treklair.fr", NOW, 60);
        assert!(codec.verify(&token, NOW + 61).is_none(), "expired token rejected");
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let codec = SessionCodec::new(SECRET);
        let token = codec.sign("admin@treklair.fr", NOW, 60);
        let other = SessionCodec::new("another-secret");
        assert!(other.verify(&token, NOW).is_none(), "cross-secret rejected");
    }

    #[test]
    fn garbage_tokens_are_rejected() {
        let codec = SessionCodec::new(SECRET);
        for bad in ["", "a.b.c.d", "abc", "###.###.###", "...."] {
            assert!(codec.verify(bad, NOW).is_none(), "garbage {bad:?} rejected");
        }
    }
}

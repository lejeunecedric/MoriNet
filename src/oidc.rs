//! Phase 17 — generic OIDC SSO (SSO-01..06).

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use rsa::pkcs1v15::SigningKey;
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::signature::{SignatureEncoding, Signer};
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Fixed test-only RSA-2048 keypairs (NOT secrets — committed test fixtures).
    /// `sso_wrong_key.pem` signs the "foreign key" rejection tests.
    const TEST_KEY_PEM: &str = include_str!("testdata/sso_test_key.pem");
    const WRONG_KEY_PEM: &str = include_str!("testdata/sso_wrong_key.pem");
    const TEST_KID: &str = "test-key-1";
    const TEST_CLIENT_ID: &str = "test-client";
    const TEST_CLIENT_SECRET: &str = "test-secret";

    /// Sign `claims` as an RS256 JWS with `pem` (kid pinned to `TEST_KID`).
    fn sign_rs256(pem: &str, claims: &Value) -> String {
        let key = RsaPrivateKey::from_pkcs8_pem(pem).expect("test key parses");
        let signing_key = SigningKey::<Sha256>::new(key);
        let header = json!({ "alg": "RS256", "kid": TEST_KID });
        let h = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header.to_string());
        let p = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
        let msg = format!("{h}.{p}");
        let sig: rsa::pkcs1v15::Signature = signing_key.sign(msg.as_bytes());
        format!(
            "{msg}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig.to_bytes())
        )
    }

    /// Forge an ID token with the legitimate test key (tests set iss/aud/nonce freely).
    fn make_id_token(claims: Value) -> String {
        sign_rs256(TEST_KEY_PEM, &claims)
    }

    fn sha256_b64url(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    }

    fn rsa_public_jwk(pem: &str) -> Value {
        let key = RsaPrivateKey::from_pkcs8_pem(pem).expect("test key parses");
        json!({
            "kty": "RSA",
            "kid": TEST_KID,
            "use": "sig",
            "alg": "RS256",
            "n": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()),
            "e": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.e().to_bytes_be()),
        })
    }

    /// Minimal in-test OIDC provider on plain HTTP (PATTERNS §8 options 1+3):
    /// discovery + JWKS + token endpoint, no TLS, no mocking crate.
    struct MockIdp {
        addr: SocketAddr,
        issuer: String,
        jwks_json: String,
        /// ID-token claims the /token endpoint signs into the response.
        claims: Arc<Mutex<Value>>,
        /// Expected S256 code challenge (test pushes it from the /sso/login redirect).
        challenge: Arc<Mutex<Option<String>>>,
        /// Last code_verifier received at /token (PKCE-arrival assertion).
        last_verifier: Arc<Mutex<Option<String>>>,
        /// Which key /token signs with (switch to WRONG_KEY_PEM for forged-token tests).
        sign_key: Arc<Mutex<&'static str>>,
    }

    impl MockIdp {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let issuer = format!("http://{addr}");
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let idp = Self {
                addr,
                issuer: issuer.clone(),
                jwks_json: json!({ "keys": [rsa_public_jwk(TEST_KEY_PEM)] }).to_string(),
                claims: Arc::new(Mutex::new(json!({
                    "iss": issuer,
                    "aud": [TEST_CLIENT_ID],
                    "sub": "sso-user-1",
                    "nonce": "unused",
                    "iat": now,
                    "exp": now + 300,
                }))),
                challenge: Arc::new(Mutex::new(None)),
                last_verifier: Arc::new(Mutex::new(None)),
                sign_key: Arc::new(Mutex::new(TEST_KEY_PEM)),
            };

            let (claims, challenge, last_verifier, sign_key, jwks) = (
                idp.claims.clone(),
                idp.challenge.clone(),
                idp.last_verifier.clone(),
                idp.sign_key.clone(),
                idp.jwks_json.clone(),
            );
            tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let (claims, challenge, last_verifier, sign_key, jwks) = (
                        claims.clone(),
                        challenge.clone(),
                        last_verifier.clone(),
                        sign_key.clone(),
                        jwks.clone(),
                    );
                    tokio::spawn(async move {
                        let _ = serve(
                            &mut stream,
                            jwks,
                            claims,
                            challenge,
                            last_verifier,
                            sign_key,
                        )
                        .await;
                    });
                }
            });
            idp
        }

        fn set_claims(&self, claims: Value) {
            *self.claims.lock().unwrap() = claims;
        }

        fn set_challenge(&self, challenge: &str) {
            *self.challenge.lock().unwrap() = Some(challenge.to_string());
        }

        fn last_verifier(&self) -> Option<String> {
            self.last_verifier.lock().unwrap().clone()
        }

        /// Sign future ID tokens with the foreign key (JWKS still advertises the real one).
        fn sign_with_foreign_key(&self) {
            *self.sign_key.lock().unwrap() = WRONG_KEY_PEM;
        }
    }

    async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
        let resp = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes()).await;
    }

    async fn serve(
        stream: &mut TcpStream,
        jwks_json: String,
        claims: Arc<Mutex<Value>>,
        challenge: Arc<Mutex<Option<String>>>,
        last_verifier: Arc<Mutex<Option<String>>>,
        sign_key: Arc<Mutex<&'static str>>,
    ) -> std::io::Result<()> {
        // Read request head + headers (minimal HTTP parse, no deps).
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await? == 0 {
                return Ok(());
            }
            buf.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&buf).to_string();
        let mut lines = head.lines();
        let mut parts = lines.next().unwrap_or("").split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();
        let content_length = lines
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            stream.read_exact(&mut body).await?;
        }
        let body = String::from_utf8_lossy(&body).to_string();

        match (method.as_str(), path.as_str()) {
            ("GET", "/.well-known/openid-configuration") => {
                // Issuer from the Host header keeps discovery self-consistent.
                let host = head
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("host"))
                    .map(|(_, v)| v.trim().to_string())
                    .unwrap_or_else(|| "127.0.0.1".to_string());
                let issuer = format!("http://{host}");
                let disc = json!({
                    "issuer": issuer,
                    "authorization_endpoint": format!("{issuer}/authorize"),
                    "token_endpoint": format!("{issuer}/token"),
                    "jwks_uri": format!("{issuer}/jwks"),
                    "response_types_supported": ["code"],
                    "subject_types_supported": ["public"],
                    "id_token_signing_alg_values_supported": ["RS256"],
                    "scopes_supported": ["openid", "email", "profile"],
                    "token_endpoint_auth_methods_supported": ["client_secret_basic"],
                    "code_challenge_methods_supported": ["S256"],
                });
                respond(stream, "200 OK", &disc.to_string()).await;
            }
            ("GET", "/jwks") => respond(stream, "200 OK", &jwks_json).await,
            ("POST", "/token") => {
                let expected_auth = format!(
                    "Basic {}",
                    STANDARD.encode(format!("{TEST_CLIENT_ID}:{TEST_CLIENT_SECRET}"))
                );
                let auth_ok = head
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .any(|(k, v)| {
                        k.eq_ignore_ascii_case("authorization") && v.trim() == expected_auth
                    });
                if !auth_ok {
                    respond(stream, "401 Unauthorized", r#"{"error":"invalid_client"}"#).await;
                    return Ok(());
                }
                let form: Vec<(String, String)> = url_decode_form(&body);
                let get = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
                let verifier = match get("code_verifier") {
                    Some(v) => v,
                    None => {
                        respond(
                            stream,
                            "400 Bad Request",
                            r#"{"error":"invalid_request","error_description":"missing code_verifier"}"#,
                        )
                        .await;
                        return Ok(());
                    }
                };
                *last_verifier.lock().unwrap() = Some(verifier.clone());
                let stored = challenge.lock().unwrap().clone();
                match stored {
                    // PKCE gate: the verifier must hash to the challenge the test stored.
                    Some(c) if sha256_b64url(&verifier) == c => {}
                    _ => {
                        respond(
                            stream,
                            "400 Bad Request",
                            r#"{"error":"invalid_grant","error_description":"pkce failure"}"#,
                        )
                        .await;
                        return Ok(());
                    }
                }
                let id_token =
                    sign_rs256(*sign_key.lock().unwrap(), &claims.lock().unwrap().clone());
                let tok = json!({
                    "access_token": "opaque",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "id_token": id_token,
                });
                respond(stream, "200 OK", &tok.to_string()).await;
            }
            _ => respond(stream, "404 Not Found", r#"{"error":"not_found"}"#).await,
        }
        Ok(())
    }

    /// Percent-decoding form parser (application/x-www-form-urlencoded).
    fn url_decode_form(body: &str) -> Vec<(String, String)> {
        body.split('&')
            .filter(|s| !s.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                (percent_decode(k), percent_decode(v))
            })
            .collect()
    }

    fn percent_decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'%' if i + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                    out.push(u8::from_str_radix(hex, 16).unwrap_or(0));
                    i += 3;
                }
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).to_string()
    }

    /// OidcRuntime pointing at a running mock IdP (client creds match the mock).
    fn oidc_cfg(issuer: &str) -> crate::config::OidcRuntime {
        crate::config::OidcRuntime {
            issuer: issuer.to_string(),
            client_id: TEST_CLIENT_ID.to_string(),
            client_secret: TEST_CLIENT_SECRET.to_string(),
            allowed_email_domains: vec![],
            groups_claim: "groups".to_string(),
            group_map: std::collections::HashMap::new(),
            sso_label: "SSO".to_string(),
        }
    }

    /// Router with `[oidc]` enabled against `issuer` — mirrors server.rs `test_router`.
    async fn test_router_with_oidc(issuer: &str) -> (axum::Router, crate::server::SharedState) {
        use crate::email::{EmailSender, InMemorySender};
        use crate::server::{build_router_with_email, AppState};
        use crate::session::SessionCodec;
        use crate::store::Store;
        use std::sync::Arc;

        let cfg = crate::config::RuntimeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            base_url: "http://localhost:8080".to_string(),
            cookie_secret: b"test-secret-0123456789abcdef".to_vec(),
            cookie_name: "morinet_session".to_string(),
            admin_email: "admin@example.com".to_string(),
            admin_password: "test-pass-9f3a".to_string(),
            smtp: Default::default(),
            data_dir: std::env::temp_dir().join("morinet-test-data"),
            vault_book_dir: None,
            locked_accounts: Vec::new(),
            oidc: Some(oidc_cfg(issuer)),
        };
        let store = Store::connect_memory().await.expect("memory store");
        store
            .ensure_admin("admin@example.com", "test-pass-9f3a")
            .await
            .unwrap();
        let email: Arc<dyn EmailSender> = Arc::new(InMemorySender::new());
        let state: crate::server::SharedState = Arc::new(AppState {
            codec: SessionCodec::new(cfg.cookie_secret.clone()),
            cfg,
            store,
            email,
            vault_locks: std::sync::Mutex::new(std::collections::HashMap::new()),
            api_rate: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        (
            build_router_with_email(state.cfg.clone(), state.store.clone(), state.email.clone()),
            state,
        )
    }

    /// Raw HTTP/1.0 GET against the mock IdP (no client deps): returns the body.
    async fn http_get(addr: SocketAddr, path: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let req = format!("GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n");
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8(raw).unwrap();
        let (head, body) = text
            .split_once("\r\n\r\n")
            .expect("response has header block");
        assert!(
            head.starts_with("HTTP/1.1 200 OK"),
            "unexpected status: {head}"
        );
        body.to_string()
    }

    #[tokio::test]
    async fn mock_idp_serves_discovery_and_jwks() {
        let idp = MockIdp::start().await;

        let disc: Value =
            serde_json::from_str(&http_get(idp.addr, "/.well-known/openid-configuration").await)
                .expect("discovery is JSON");
        assert_eq!(disc["issuer"], idp.issuer.as_str());
        assert_eq!(
            disc["authorization_endpoint"],
            format!("{}/authorize", idp.issuer)
        );
        assert_eq!(disc["token_endpoint"], format!("{}/token", idp.issuer));
        assert_eq!(disc["jwks_uri"], format!("{}/jwks", idp.issuer));
        assert_eq!(disc["code_challenge_methods_supported"][0], "S256");
        assert_eq!(disc["id_token_signing_alg_values_supported"][0], "RS256");

        let jwks: Value =
            serde_json::from_str(&http_get(idp.addr, "/jwks").await).expect("jwks is JSON");
        let key = &jwks["keys"][0];
        assert_eq!(key["kid"], "test-key-1");
        assert_eq!(key["kty"], "RSA");
        assert_eq!(key["alg"], "RS256");
        assert!(
            key["n"].as_str().unwrap().len() > 200,
            "modulus is base64url"
        );
        assert_eq!(key["e"], "AQAB");
    }
}

//! Phase 17 — generic OIDC SSO (SSO-01..06).
//!
//! Authorization-code + PKCE (S256) flow with `state` + `nonce` (D-01). The
//! state trio rides in one short-lived signed cookie (`morinet_sso_state`,
//! minted with the existing [`SessionCodec`], TTL 600 s) — no server-side
//! state table. The success path ends byte-identical to `login_submit`
//! (D-02): same HMAC session cookie, same attributes, so every downstream
//! gate stays untouched. Identity linking/provisioning is wave 3: an unknown
//! `(iss, sub)` refuses fail-closed here.

use axum::extract::{Query, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClaimsVerificationError, ClientId, ClientSecret,
    CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
};
use serde::Deserialize;
use subtle::ConstantTimeEq;

use crate::server::{error_page, parse_cookies, unix_now, SharedState};
use crate::session::DEFAULT_TTL_SECS;

/// Short-lived cookie carrying `state\nnonce\ncode_verifier` (signed SessionCodec payload).
const SSO_STATE_COOKIE: &str = "morinet_sso_state";
/// Replay window for the in-flight authorization (planner decision, PATTERNS §11).
const SSO_STATE_TTL_SECS: u64 = 600;

fn random_token() -> String {
    // 64 hex chars: valid PKCE verifier length (43..=128) as well as state/nonce.
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn not_found() -> Response {
    let mut r = error_page("SSO is not configured.");
    *r.status_mut() = StatusCode::NOT_FOUND;
    r
}

/// 403 refusal + `deny` audit row (best-effort, never breaks the refusal).
async fn deny(state: &SharedState, iss: &str, sub: &str, detail: &str) -> Response {
    let _ = state
        .store
        .log_auth_event("deny", "", iss, sub, detail)
        .await;
    let mut r = error_page("Sign-in could not be completed.");
    *r.status_mut() = StatusCode::FORBIDDEN;
    r
}

/// `Set-Cookie` clearing the state cookie (always sent, success or refusal).
fn clear_state_cookie() -> String {
    format!("{SSO_STATE_COOKIE}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0")
}

/// GET /sso/login — build the authorization URL (PKCE S256 + state + nonce)
/// and stash the trio in the signed state cookie.
pub(crate) async fn sso_login(State(state): State<SharedState>) -> Response {
    let Some(oidc) = &state.cfg.oidc else {
        return not_found();
    };
    let http = openidconnect::reqwest::Client::new();
    let issuer = match IssuerUrl::new(oidc.issuer.clone()) {
        Ok(i) => i,
        Err(_) => return not_found(),
    };
    let provider = match CoreProviderMetadata::discover_async(issuer.clone(), &http).await {
        Ok(p) => p,
        Err(_) => return error_page("SSO provider unreachable."),
    };
    let client = CoreClient::from_provider_metadata(
        provider,
        ClientId::new(oidc.client_id.clone()),
        Some(ClientSecret::new(oidc.client_secret.clone())),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/sso/callback", state.cfg.base_url))
            .expect("static redirect URI parses"),
    );
    let state_token = random_token();
    let nonce = random_token();
    let verifier = PkceCodeVerifier::new(random_token());
    let st_for_url = state_token.clone();
    let nonce_for_url = nonce.clone();
    let (auth_url, _csrf, _nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            move || CsrfToken::new(st_for_url),
            move || Nonce::new(nonce_for_url),
        )
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(PkceCodeChallenge::from_code_verifier_sha256(&verifier))
        .url();
    // state + nonce + verifier in ONE signed short-lived cookie (PATTERNS §11).
    let payload = format!("{state_token}\n{nonce}\n{}", verifier.secret());
    let cookie = format!(
        "{SSO_STATE_COOKIE}={}; HttpOnly; Path=/; SameSite=Lax; Max-Age={SSO_STATE_TTL_SECS}",
        state.codec.sign(&payload, unix_now(), SSO_STATE_TTL_SECS)
    );
    // 302 Found (axum's Redirect has no 302 constructor).
    (
        StatusCode::FOUND,
        [
            (LOCATION, auth_url.as_str().to_string()),
            (SET_COOKIE, cookie),
        ],
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub(crate) struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// GET /sso/callback — fail-closed validation (state, nonce, JWKS signature,
/// iss/aud/exp), then the byte-identical session mint (D-02).
pub(crate) async fn sso_callback(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<CallbackParams>,
) -> Response {
    let Some(oidc) = &state.cfg.oidc else {
        return not_found();
    };
    let iss = oidc.issuer.clone();
    // IdP-reported error (user denied consent, etc.) — refuse, no session.
    if q.error.is_some() {
        let mut r = deny(&state, &iss, "", "idp_error").await;
        r.headers_mut()
            .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
        return r;
    }
    // State cookie: signed, TTL-enforced by the codec.
    let stored = headers
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_cookies)
        .and_then(|c| c.get(SSO_STATE_COOKIE).cloned())
        .and_then(|token| state.codec.verify(&token, unix_now()))
        .and_then(|claims| {
            let mut parts = claims.email.splitn(3, '\n');
            Some((
                parts.next()?.to_string(),
                parts.next()?.to_string(),
                parts.next()?.to_string(),
            ))
        });
    let (expected_state, expected_nonce, expected_verifier) = match stored {
        Some(t) => t,
        None => {
            let mut r = deny(&state, &iss, "", "state_invalid").await;
            r.headers_mut()
                .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
            return r;
        }
    };
    // Constant-time state compare (T-17-05).
    let state_ok = q
        .state
        .as_deref()
        .map(|s| bool::from(s.as_bytes().ct_eq(expected_state.as_bytes())))
        .unwrap_or(false);
    let code = match (&q.code, state_ok) {
        (Some(code), true) => code.clone(),
        _ => {
            let mut r = deny(&state, &iss, "", "state_invalid").await;
            r.headers_mut()
                .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
            return r;
        }
    };

    // Token exchange + ID-token validation (discovery + JWKS, iss/aud/nonce/exp).
    let sub =
        match exchange_and_verify(&state, oidc, &code, &expected_verifier, &expected_nonce).await {
            Ok(sub) => sub,
            Err(detail) => {
                let mut r = deny(&state, &iss, "", &detail).await;
                r.headers_mut()
                    .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
                return r;
            }
        };
    // Identity resolution: (iss, sub) is authoritative (D-03). Wave 3 replaces
    // the not_linked refusal with linking/provisioning.
    match state
        .store
        .user_by_provider_iss_sub("oidc", &iss, &sub)
        .await
        .ok()
        .flatten()
    {
        Some((_id, email, _role)) => {
            // §2b verbatim (D-02): same mint, same cookie attributes as login_submit.
            let token = state.codec.sign(&email, unix_now(), DEFAULT_TTL_SECS);
            let session_cookie = format!(
                "{}={token}; HttpOnly; Path=/; SameSite=Lax",
                state.cfg.cookie_name
            );
            // Two Set-Cookie headers must APPEND (axum array parts use insert).
            let mut resp = Redirect::to("/").into_response();
            resp.headers_mut()
                .append(SET_COOKIE, session_cookie.parse().unwrap());
            resp.headers_mut()
                .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
            resp
        }
        None => {
            let mut r = deny(&state, &iss, &sub, "not_linked").await;
            *r.body_mut() = axum::body::Body::from("This account is not linked yet.".to_string());
            r.headers_mut()
                .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
            r
        }
    }
}

/// Exchange the code (PKCE verifier + Basic client auth) and validate the
/// JWKS-signed ID token. Returns the authenticated `sub`.
async fn exchange_and_verify(
    state: &SharedState,
    oidc: &crate::config::OidcRuntime,
    code: &str,
    verifier: &str,
    nonce: &str,
) -> Result<String, String> {
    let _ = state;
    let http = openidconnect::reqwest::Client::new();
    let issuer = IssuerUrl::new(oidc.issuer.clone()).map_err(|_| "token_invalid".to_string())?;
    let provider = CoreProviderMetadata::discover_async(issuer.clone(), &http)
        .await
        .map_err(|_| "token_invalid".to_string())?;
    let client = CoreClient::from_provider_metadata(
        provider,
        ClientId::new(oidc.client_id.clone()),
        Some(ClientSecret::new(oidc.client_secret.clone())),
    );
    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|_| "token_invalid".to_string())?
        .set_pkce_verifier(PkceCodeVerifier::new(verifier.to_string()))
        .request_async(&http)
        .await
        .map_err(|_| "token_invalid".to_string())?;
    let id_token = token_response
        .id_token()
        .ok_or("token_invalid".to_string())?;
    let nonce = Nonce::new(nonce.to_string());
    let claims = id_token
        .claims(&client.id_token_verifier(), &nonce)
        .map_err(|e| match e {
            ClaimsVerificationError::InvalidNonce(_) => "nonce_mismatch".to_string(),
            _ => "token_invalid".to_string(),
        })?;
    let sub = claims.subject().to_string();
    // Access token is NEVER consulted for identity (D-15 groundwork); only the
    // signature-validated ID token feeds identity resolution.
    Ok(sub)
}

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
        test_router_with_cfg(Some(oidc_cfg(issuer))).await
    }

    /// Router with `[oidc]` absent — password-login-only baseline.
    async fn test_router_disabled() -> (axum::Router, crate::server::SharedState) {
        test_router_with_cfg(None).await
    }

    async fn test_router_with_cfg(
        oidc: Option<crate::config::OidcRuntime>,
    ) -> (axum::Router, crate::server::SharedState) {
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
            oidc,
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

    // ---------- SSO core flow (plan 17-02 task 3) ----------

    use axum::body::Body;
    use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
    use axum::http::{HeaderMap, Request, StatusCode};
    use axum::response::Response;
    use tower::ServiceExt; // oneshot

    async fn req(
        app: axum::Router,
        method: &str,
        uri: &str,
        headers: HeaderMap,
        body: Option<Body>,
    ) -> Response {
        let mut builder = Request::builder().method(method).uri(uri);
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }
        app.oneshot(builder.body(body.unwrap_or_else(Body::empty)).unwrap())
            .await
            .unwrap()
    }

    fn set_cookie_headers(res: &Response) -> Vec<String> {
        res.headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect()
    }

    fn find_cookie(cookies: &[String], name: &str) -> Option<String> {
        cookies
            .iter()
            .find(|c| c.starts_with(&format!("{name}=")))
            .cloned()
    }

    /// Query param of a URL (mock IdP values are uuid/base64url — no percent chars).
    fn url_query_param(url: &str, key: &str) -> Option<String> {
        let query = url.split_once('?').map(|(_, q)| q)?;
        query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == key).then(|| percent_decode(v))
        })
    }

    /// Drive GET /sso/login: returns (Location, sso_state cookie value).
    async fn drive_sso_login(app: &axum::Router) -> (String, String) {
        let res = req(app.clone(), "GET", "/sso/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::FOUND, "auth URL redirect");
        let loc = res
            .headers()
            .get(LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let cookie = find_cookie(&set_cookie_headers(&res), "morinet_sso_state")
            .expect("state cookie set on /sso/login");
        let value = cookie
            .split(';')
            .next()
            .unwrap()
            .trim_start_matches("morinet_sso_state=")
            .to_string();
        (loc, value)
    }

    /// Decode the signed state cookie into (state, nonce, code_verifier).
    fn decode_state_cookie(
        state: &crate::server::SharedState,
        value: &str,
    ) -> (String, String, String) {
        let claims = state
            .codec
            .verify(value, crate::server::unix_now())
            .expect("state cookie verifies");
        let parts: Vec<&str> = claims.email.splitn(3, '\n').collect();
        assert_eq!(parts.len(), 3, "payload is state\\nnonce\\nverifier");
        (
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        )
    }

    /// Full happy-path setup: linked user + mock configured with challenge + claims.
    async fn happy_setup(
        idp: &MockIdp,
        state: &crate::server::SharedState,
        loc: &str,
        cookie_value: &str,
        sub: &str,
    ) -> (String, String, String) {
        let challenge = url_query_param(loc, "code_challenge").expect("code_challenge in auth URL");
        idp.set_challenge(&challenge);
        let (s, n, v) = decode_state_cookie(state, cookie_value);
        let now = crate::server::unix_now();
        idp.set_claims(json!({
            "iss": idp.issuer,
            "aud": [TEST_CLIENT_ID],
            "sub": sub,
            "nonce": n,
            "iat": now,
            "exp": now + 300,
        }));
        (s, n, v)
    }

    async fn callback(app: &axum::Router, query: &str, state_cookie: Option<&str>) -> Response {
        let mut headers = HeaderMap::new();
        if let Some(c) = state_cookie {
            headers.insert(COOKIE, format!("morinet_sso_state={c}").parse().unwrap());
        }
        req(
            app.clone(),
            "GET",
            &format!("/sso/callback?{query}"),
            headers,
            None,
        )
        .await
    }

    async fn seed_linked_user(
        state: &crate::server::SharedState,
        email: &str,
        iss: &str,
        sub: &str,
    ) {
        state
            .store
            .create_user(email, "pw-not-used", "reader")
            .await
            .unwrap();
        let uid = state.store.user_id_for(email).await.unwrap().unwrap();
        state
            .store
            .link_identity("oidc", iss, sub, uid)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn sso_login_disabled_returns_404() {
        let (app, _) = test_router_disabled().await;
        let res = req(app, "GET", "/sso/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sso_login_redirects_with_pkce_s256_and_state_cookie() {
        let idp = MockIdp::start().await;
        let (app, _) = test_router_with_oidc(&idp.issuer).await;
        let res = req(app, "GET", "/sso/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::FOUND);
        let loc = res
            .headers()
            .get(LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(loc.starts_with(&format!("{}/authorize?", idp.issuer)));
        assert_eq!(
            url_query_param(&loc, "response_type").as_deref(),
            Some("code")
        );
        assert_eq!(
            url_query_param(&loc, "code_challenge_method").as_deref(),
            Some("S256")
        );
        assert!(url_query_param(&loc, "code_challenge").is_some());
        assert!(url_query_param(&loc, "state").is_some());
        assert!(url_query_param(&loc, "nonce").is_some());
        assert_eq!(
            url_query_param(&loc, "redirect_uri").as_deref(),
            Some("http://localhost:8080/sso/callback")
        );
        let scope = url_query_param(&loc, "scope").unwrap();
        for s in ["openid", "email", "profile"] {
            assert!(
                scope.split(['+', ' ']).any(|x| x == s),
                "scope missing {s}: {scope}"
            );
        }
        let cookies = set_cookie_headers(&res);
        let cookie = find_cookie(&cookies, "morinet_sso_state").expect("state cookie");
        assert!(cookie.contains("HttpOnly"), "{cookie}");
        assert!(cookie.contains("Path=/"), "{cookie}");
        assert!(cookie.contains("SameSite=Lax"), "{cookie}");
        assert!(cookie.contains("Max-Age=600"), "{cookie}");
    }

    #[tokio::test]
    async fn sso_callback_happy_path_mints_session_cookie() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get(LOCATION).unwrap(), "/");
        let cookies = set_cookie_headers(&res);
        let session = find_cookie(&cookies, "morinet_session")
            .unwrap_or_else(|| panic!("session cookie minted; got {cookies:?}"));
        let token = session
            .split(';')
            .next()
            .unwrap()
            .trim_start_matches("morinet_session=");
        let claims = state
            .codec
            .verify(token, crate::server::unix_now())
            .expect("session verifies");
        assert_eq!(claims.email, "sso-user@example.com");
        // state cookie is always cleared
        let cleared = find_cookie(&cookies, "morinet_sso_state").expect("state cookie cleared");
        assert!(cleared.contains("Max-Age=0"), "{cleared}");
    }

    #[tokio::test]
    async fn sso_callback_cookie_byte_identical_to_password_login() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        // password login cookie (server.rs §2b reference)
        let mut form_headers = HeaderMap::new();
        form_headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let pw = req(
            app.clone(),
            "POST",
            "/login",
            form_headers,
            Some(Body::from(
                "email=admin%40example.com&password=test-pass-9f3a",
            )),
        )
        .await;
        let pw_cookie = find_cookie(&set_cookie_headers(&pw), "morinet_session")
            .expect("password session cookie");
        // SSO callback cookie
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        let sso_cookie =
            find_cookie(&set_cookie_headers(&res), "morinet_session").expect("sso session cookie");
        // Byte-identical SHAPE: same name prefix + same attribute string (D-02).
        let shape = |c: &str| {
            let token = c.split(';').next().unwrap();
            format!(
                "morinet_session=;{}",
                c[token.len()..].trim_start_matches("morinet_session=")
            )
        };
        assert_eq!(shape(&pw_cookie), shape(&sso_cookie));
        assert!(
            sso_cookie.ends_with("; HttpOnly; Path=/; SameSite=Lax"),
            "{sso_cookie}"
        );
    }

    #[tokio::test]
    async fn sso_callback_rejects_missing_state_cookie() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let res = callback(&app, "code=test-code&state=whatever", None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_rejects_forged_state_cookie() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (_loc, cookie_value) = drive_sso_login(&app).await;
        // Sign with a DIFFERENT secret — the codec verify must refuse it.
        let forged = crate::session::SessionCodec::new("attacker-secret").sign(
            "state\nnonce\nverifier",
            crate::server::unix_now(),
            600,
        );
        assert_ne!(forged, cookie_value);
        let res = callback(&app, "code=test-code&state=whatever", Some(&forged)).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_rejects_state_mismatch() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (_s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let res = callback(
            &app,
            "code=test-code&state=tampered-state",
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_rejects_replayed_callback() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let first = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(first.status(), StatusCode::SEE_OTHER);
        // Replay WITHOUT the (cleared) state cookie — single-use falls out of the clear.
        let replay = callback(&app, &format!("code=test-code&state={s}"), None).await;
        assert_eq!(replay.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&replay), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_rejects_nonce_mismatch() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        // Overwrite claims with a WRONG nonce (IdP misbehaving / token mix-up).
        let now = crate::server::unix_now();
        idp.set_claims(json!({
            "iss": idp.issuer, "aud": [TEST_CLIENT_ID], "sub": "sso-user-1",
            "nonce": "attacker-nonce", "iat": now, "exp": now + 300,
        }));
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_rejects_wrong_signing_key() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        idp.sign_with_foreign_key();
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_rejects_bad_aud() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let now = crate::server::unix_now();
        idp.set_claims(json!({
            "iss": idp.issuer, "aud": ["someone-else"], "sub": "sso-user-1",
            "nonce": n, "iat": now, "exp": now + 300,
        }));
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
    }

    #[tokio::test]
    async fn sso_callback_unknown_identity_refuses_and_logs() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        // NO link_identity row — wave-3 provisioning replaces this refusal.
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
        let denies = state
            .store
            .count_auth_events("deny", "not_linked")
            .await
            .unwrap();
        assert_eq!(denies, 1, "deny event logged with detail not_linked");
    }

    #[tokio::test]
    async fn sso_callback_requires_pkce_verifier() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::SEE_OTHER,
            "happy path proves verifier flow"
        );
        // The mock refuses any exchange whose verifier does not hash to the challenge,
        // so a successful exchange proves the verifier arrived intact.
        assert_eq!(idp.last_verifier().as_deref(), Some(v.as_str()));
    }

    async fn body_text(res: Response) -> String {
        let bytes = axum::body::to_bytes(res.into_body(), 4_194_304)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn login_page_no_sso_ui_when_oidc_absent() {
        let (app, _) = test_router_disabled().await;
        let res = req(app, "GET", "/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(!body.contains("/sso/login"), "{body}");
        assert_eq!(body, crate::server::LOGIN_HTML);
    }

    #[tokio::test]
    async fn login_page_shows_sso_button_when_oidc_configured() {
        let (app, _) = test_router_with_oidc("http://idp.invalid").await;
        let res = req(app, "GET", "/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("href=\"/sso/login\""), "{body}");
        assert!(body.contains("Se connecter via SSO"), "{body}");
        assert!(body.contains("<p>ou</p>"), "{body}");
        assert!(
            body.find("href=\"/sso/login\"").unwrap() < body.find("<form").unwrap(),
            "SSO button must appear before the password form"
        );
    }

    #[tokio::test]
    async fn login_page_sso_label_configurable_and_escaped() {
        let mut cfg = oidc_cfg("http://idp.invalid");
        cfg.sso_label = "<script>x</script>".to_string();
        let (app, _) = test_router_with_cfg(Some(cfg)).await;
        let res = req(app, "GET", "/login", HeaderMap::new(), None).await;
        let body = body_text(res).await;
        assert!(!body.contains("<script>x</script>"), "{body}");
        assert!(body.contains("&lt;script&gt;"), "{body}");
    }

    #[tokio::test]
    async fn login_submit_failure_page_keeps_sso_button() {
        let (app, _) = test_router_with_oidc("http://idp.invalid").await;
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app,
            "POST",
            "/login",
            headers,
            Some(Body::from("email=admin%40example.com&password=wrong-pass")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let body = body_text(res).await;
        assert!(body.contains("href=\"/sso/login\""), "{body}");
    }
}

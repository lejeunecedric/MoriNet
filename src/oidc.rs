//! Phase 17 — generic OIDC SSO (SSO-01..06).
//!
//! Authorization-code + PKCE (S256) flow with `state` + `nonce` (D-01). The
//! state trio rides in one short-lived signed cookie (`morinet_sso_state`,
//! minted with the existing [`SessionCodec`], TTL 600 s) — no server-side
//! state table. The success path ends byte-identical to `login_submit`
//! (D-02): same HMAC session cookie, same attributes, so every downstream
//! gate stays untouched. Identity resolution (wave 3): `(iss, sub)` is the
//! authoritative key; first logins auto-link on verified email (readers
//! only) or auto-provision on an allowed domain — fail-closed everywhere.

use axum::extract::{Query, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreClient, CoreErrorResponseType, CoreGenderClaim,
    CoreJsonWebKey, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreResponseType, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse,
};
use openidconnect::{
    AdditionalClaims, AuthenticationFlow, AuthorizationCode, ClaimsVerificationError, ClientId,
    ClientSecret, CsrfToken, EmptyExtraTokenFields, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, StandardErrorResponse, StandardTokenResponse,
    TokenResponse,
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::server::{error_page, parse_cookies, unix_now, SharedState};

/// Unregistered ID-token claims captured DURING signature verification
/// (D-15): the flatten map receives every claim the Core types do not
/// consume — the groups claim lives here and nowhere else. The access token
/// and userinfo are never parsed, so a claim smuggled outside the verified
/// ID token cannot influence group sync (T-17-16).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct OidcExtraClaims {
    #[serde(flatten)]
    map: std::collections::HashMap<String, serde_json::Value>,
}
impl AdditionalClaims for OidcExtraClaims {}

/// Token response whose ID token carries [`OidcExtraClaims`] (Core crypto
/// unchanged — only the additional-claims type parameter differs).
type OidcIdTokenFields = openidconnect::IdTokenFields<
    OidcExtraClaims,
    EmptyExtraTokenFields,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
>;
type OidcTokenResponse =
    StandardTokenResponse<OidcIdTokenFields, openidconnect::core::CoreTokenType>;
type OidcClient = openidconnect::Client<
    OidcExtraClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    OidcTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    openidconnect::EndpointSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointNotSet,
    openidconnect::EndpointMaybeSet,
    openidconnect::EndpointMaybeSet,
>;

/// Short-lived cookie carrying `state\nnonce\ncode_verifier` (signed SessionCodec payload).
const SSO_STATE_COOKIE: &str = "morinet_sso_state";
/// Replay window for the in-flight authorization (planner decision, PATTERNS §11).
const SSO_STATE_TTL_SECS: u64 = 600;
/// Identity provider key in `user_identities` — single-IdP v1; the column is
/// ready for future providers (D-11).
const OIDC_PROVIDER: &str = "oidc";

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

/// REV-17-04: serializes in-process group sync so the last-admin
/// check-then-act guard cannot be raced by two concurrent SSO logins.
static SYNC_GROUPS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        // REV-17-01: pre-parse refusals are NOT audited — every anonymous
        // request could otherwise insert an auth_events row (flood vector).
        let mut r = error_page("Sign-in could not be completed.");
        *r.status_mut() = StatusCode::FORBIDDEN;
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
            // REV-17-01: pre-parse refusal, not audited (flood vector).
            let mut r = error_page("Sign-in could not be completed.");
            *r.status_mut() = StatusCode::FORBIDDEN;
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
    let (claims, extra) =
        match exchange_and_verify(&state, oidc, &code, &expected_verifier, &expected_nonce).await {
            Ok(pair) => pair,
            Err(detail) => {
                let mut r = deny(&state, &iss, "", &detail).await;
                r.headers_mut()
                    .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
                return r;
            }
        };
    // Identity resolution (wave 3): LOCKED order — (iss,sub) hit → sign in;
    // verified-email auto-link (readers only); domain-gated auto-provision.
    // All three success paths converge on the SAME mint below (D-02).
    match resolve_identity(&state, oidc, &iss, &claims).await {
        Ok(email) => {
            // §2b verbatim (D-02): same mint, same cookie attributes as login_submit.
            // Fully qualified so the TTL constant appears exactly once in this
            // file — the single session-mint site gate (D-02, no drift).
            let token = state
                .codec
                .sign(&email, unix_now(), crate::session::DEFAULT_TTL_SECS);
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
            // Group sync (D-09/D-10/D-15): best-effort AFTER the mint, before
            // the response — a sync failure never blocks sign-in (D-04
            // precedent), and every applied change is audited inside.
            if let Some(uid) = state.store.user_id_for(&email).await.ok().flatten() {
                if let Err(e) = sync_groups(&state, &email, uid, &iss, &claims.sub, &extra).await {
                    eprintln!("oidc: group sync failed for {email}: {e}");
                }
            }
            resp
        }
        Err(refusal) => {
            // Deny audit: kind deny + locked detail, actor when an account
            // was reached ('' otherwise). Never tokens or codes (T-17-04).
            let _ = state
                .store
                .log_auth_event("deny", &refusal.actor, &iss, &claims.sub, refusal.detail)
                .await;
            let mut r = error_page(refusal.message);
            *r.status_mut() = StatusCode::FORBIDDEN;
            r.headers_mut()
                .append(SET_COOKIE, clear_state_cookie().parse().unwrap());
            r
        }
    }
}

/// Claims extracted from the signature-verified ID token that feed identity
/// resolution. ONLY the validated ID token populates this — the access token
/// and userinfo are never consulted (D-15).
#[derive(Debug)]
pub(crate) struct VerifiedIdentity {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
}

/// A fail-closed resolution refusal: locked `detail` for the deny audit row,
/// `actor` (matched local email when an account was reached, '' otherwise),
/// and the generic user-facing page message.
#[derive(Debug)]
struct Refusal {
    detail: &'static str,
    actor: String,
    message: &'static str,
}

impl Refusal {
    fn deny(detail: &'static str) -> Self {
        Self {
            detail,
            actor: String::new(),
            message: "Sign-in could not be completed.",
        }
    }
}

/// Exact-domain allowlist check on the NORMALIZED email (D-04): exactly one
/// `@`, and the domain part EQUALS an allowlist entry — no suffix tricks
/// (`evilexample.com` must not match `example.com`). An empty allowlist
/// refuses everything: auto-provisioning is DISABLED when unconfigured
/// (fail-closed; linking existing accounts still works).
fn domain_allowed(email_norm: &str, allowed: &[String]) -> bool {
    let mut parts = email_norm.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && allowed
            .iter()
            .any(|d| d.trim().to_ascii_lowercase() == domain)
}

/// The LOCKED identity-resolution order (plan 17-03, D-03/D-04/D-05/D-12):
/// 1. `(iss, sub)` hit → sign in as that user; the email claim is NEVER
///    consulted for returning users (A1 avoidance).
/// 2. Miss → strict `email_verified == true` (JSON boolean) + non-empty email
///    → normalized-email lookup: admin match → refuse (no link, no sign-in);
///    reader match → auto-link + log `link`/`auto_link` + sign in. A lost
///    link race (INSERT OR IGNORE false) re-reads and proceeds as returning.
/// 3. No match → exact-domain allowlist → provision reader with an
///    unguessable placeholder + link + log `provision`/`auto_provision`;
///    otherwise refuse `domain_not_allowed`.
///
/// Returns the LOCAL email to mint the session for (never the raw claim).
async fn resolve_identity(
    state: &SharedState,
    oidc: &crate::config::OidcRuntime,
    iss: &str,
    claims: &VerifiedIdentity,
) -> Result<String, Refusal> {
    // 1. Returning user — (iss, sub) is the authoritative join key (D-03).
    if let Some((_id, email, _role)) = state
        .store
        .user_by_provider_iss_sub(OIDC_PROVIDER, iss, &claims.sub)
        .await
        .ok()
        .flatten()
    {
        return Ok(email);
    }
    // 2. First link: email_verified must be the JSON boolean true, and the
    //    email claim non-empty (anything else fails closed).
    if claims.email_verified != Some(true) {
        return Err(Refusal::deny("email_not_verified"));
    }
    let email_norm = claims
        .email
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase(); // D-12 rule (documented on user_by_email_normalized)
    if email_norm.is_empty() {
        return Err(Refusal::deny("no_email"));
    }
    // REV-17-03: control chars or embedded whitespace would provision an
    // account whose minted session verify_session then rejects — refuse
    // before any write (fail-closed, audited post-exchange).
    if email_norm
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(Refusal::deny("invalid_email"));
    }
    if let Some((uid, email, role)) = state
        .store
        .user_by_email_normalized(&email_norm)
        .await
        .ok()
        .flatten()
    {
        if role == "admin" {
            // Admin accounts are NEVER auto-linked (D-05, A1).
            return Err(Refusal {
                detail: "admin_match",
                actor: email,
                message: "This account cannot sign in via SSO.",
            });
        }
        // Reader: auto-link. INSERT OR IGNORE — a false return means a
        // concurrent login already linked this (iss,sub): re-read and
        // proceed as the returning user (T-17-14).
        match state
            .store
            .link_identity(OIDC_PROVIDER, iss, &claims.sub, uid)
            .await
        {
            Ok(true) => {}
            // REV-17-05: distinguish losing the race (row exists now) from a
            // DB error — never log a `link` event we do not own.
            Ok(false) | Err(_) => {
                match state
                    .store
                    .user_by_provider_iss_sub(OIDC_PROVIDER, iss, &claims.sub)
                    .await
                {
                    Ok(Some((_id, email, _role))) => return Ok(email),
                    Ok(None) => return Err(Refusal::deny("link_race")),
                    Err(_) => return Err(Refusal::deny("link_error")),
                }
            }
        }
        let _ = state
            .store
            .log_auth_event("link", &email, iss, &claims.sub, "auto_link")
            .await;
        return Ok(email);
    }
    // 3. Auto-provision, gated on the exact-domain allowlist (D-04, A7).
    if !domain_allowed(&email_norm, &oidc.allowed_email_domains) {
        return Err(Refusal {
            detail: "domain_not_allowed",
            actor: String::new(),
            message: "This email domain is not allowed for SSO sign-in.",
        });
    }
    if let Err(e) = state
        .store
        .create_user_without_password(&email_norm, "reader")
        .await
    {
        // Fail closed: a provisioning failure is a refusal, never a bypass.
        let _ = e;
        return Err(Refusal::deny("provision_failed"));
    }
    if let Some((uid, _, _)) = state
        .store
        .user_by_email_normalized(&email_norm)
        .await
        .ok()
        .flatten()
    {
        let _ = state
            .store
            .link_identity(OIDC_PROVIDER, iss, &claims.sub, uid)
            .await;
    }
    let _ = state
        .store
        .log_auth_event("provision", &email_norm, iss, &claims.sub, "auto_provision")
        .await;
    Ok(email_norm)
}

/// Exchange the code (PKCE verifier + Basic client auth) and validate the
/// JWKS-signed ID token. Returns the authenticated `sub` plus the `email` /
/// `email_verified` claims, and the unregistered claims captured during the
/// same signature-verified parse — all read ONLY from the signature-validated
/// ID token (D-15); the access token is never consulted for identity.
async fn exchange_and_verify(
    state: &SharedState,
    oidc: &crate::config::OidcRuntime,
    code: &str,
    verifier: &str,
    nonce: &str,
) -> Result<
    (
        VerifiedIdentity,
        std::collections::HashMap<String, serde_json::Value>,
    ),
    String,
> {
    let _ = state;
    let http = openidconnect::reqwest::Client::new();
    let issuer = IssuerUrl::new(oidc.issuer.clone()).map_err(|_| "token_invalid".to_string())?;
    let provider = CoreProviderMetadata::discover_async(issuer.clone(), &http)
        .await
        .map_err(|_| "token_invalid".to_string())?;
    let client = OidcClient::from_provider_metadata(
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
    // `email_verified` must be the JSON boolean true to count: the library
    // types it as a bool, so a string "true" fails claim parsing outright
    // (token_invalid) — fail-closed either way (plan 17-03 strictness).
    let email = claims.email().map(|e| e.as_str().to_string());
    let email_verified = claims.email_verified();
    // Unregistered claims captured by the SAME signature-verified parse
    // (D-15): only claims bound to this verified token exist here.
    let extra = claims.additional_claims().map.clone();
    Ok((
        VerifiedIdentity {
            sub,
            email,
            email_verified,
        },
        extra,
    ))
}

/// D-09/D-10/D-15 replace-on-login group sync, run best-effort after every
/// successful SSO sign-in. Locked diff algorithm (plan 17-04):
///
/// * claim ABSENT -> keep everything (early return, no audit rows);
/// * claim PRESENT-but-empty -> remove ALL managed memberships;
/// * managed set = `group_map` VALUES only — unmanaged groups are never
///   touched, unknown targets are skipped + deny-logged (never auto-created);
/// * wrong-shape claim -> skip sync + deny `groups_claim_invalid` (fail-closed);
/// * removals that would empty the `admin` group are refused (`last_admin_kept`);
/// * adds log `group_sync`/`added:{group}`, removals log `demote`/`removed:{group}`.
async fn sync_groups(
    state: &SharedState,
    user_email: &str,
    user_id: i64,
    iss: &str,
    sub: &str,
    extra: &std::collections::HashMap<String, serde_json::Value>,
) -> anyhow::Result<()> {
    use std::collections::{HashMap, HashSet};
    // REV-17-04: the last-admin guard is check-then-act; serialize syncs in
    // this single-binary process so concurrent logins cannot jointly empty
    // the admin group.
    let _sync_guard = SYNC_GROUPS_LOCK.lock().await;
    let Some(oidc) = &state.cfg.oidc else {
        return Ok(());
    };
    // Claim source: ONLY the verified ID token's unregistered claims (D-15),
    // keyed by the configurable claim name (never a hardcoded literal here).
    let claim: Vec<&str> = match extra.get(&oidc.groups_claim) {
        None => return Ok(()), // absent -> managed memberships untouched (D-09)
        Some(serde_json::Value::Array(items)) => items.iter().filter_map(|v| v.as_str()).collect(),
        Some(_) => {
            // Present but not an array: never act on garbage (T-17-16).
            state
                .store
                .log_auth_event("deny", user_email, iss, sub, "groups_claim_invalid")
                .await?;
            return Ok(());
        }
    };
    // Managed set = mapping VALUES resolved to existing groups (D-10).
    // A typo'd target is skipped + deny-logged; sync never creates groups.
    let mut managed: HashMap<String, i64> = HashMap::new();
    for name in oidc.group_map.values() {
        if managed.contains_key(name.as_str()) {
            continue; // dedup: several claim values may map to one group
        }
        match state.store.group_id_for_name(name).await? {
            Some(id) => {
                managed.insert(name.clone(), id);
            }
            None => {
                state
                    .store
                    .log_auth_event("deny", user_email, iss, sub, "unknown_group_target")
                    .await?;
            }
        }
    }
    // desired = mapped targets whose claim value is present in the claim.
    let mut desired: HashSet<i64> = HashSet::new();
    for (claim_value, target) in &oidc.group_map {
        if claim.contains(&claim_value.as_str()) {
            if let Some(id) = managed.get(target.as_str()) {
                desired.insert(*id);
            }
        }
    }
    // current = user's memberships intersected with the managed ids.
    let current_managed: Vec<(i64, String)> = state
        .store
        .group_memberships_for_user(user_id)
        .await?
        .into_iter()
        .filter(|(id, name)| managed.get(name.as_str()) == Some(id))
        .collect();
    let current_ids: HashSet<i64> = current_managed.iter().map(|(id, _)| *id).collect();
    for id in &desired {
        if current_ids.contains(id) {
            continue;
        }
        let name = managed
            .iter()
            .find(|(_, v)| *v == id)
            .map(|(k, _)| k.as_str())
            .unwrap_or("");
        state.store.add_group_member(*id, user_email).await?;
        // REV-17-06: audit is best-effort — an insert failure must not abort
        // the diff mid-way leaving applied-but-unaudited changes.
        let _ = state
            .store
            .log_auth_event("group_sync", user_email, iss, sub, &format!("added:{name}"))
            .await;
    }
    for (id, name) in &current_managed {
        if desired.contains(id) {
            continue;
        }
        // Anti-lockout (D-07 spirit): never empty the admin group via sync.
        if name == "admin" && state.store.group_members(*id).await?.len() <= 1 {
            let _ = state
                .store
                .log_auth_event("deny", user_email, iss, sub, "last_admin_kept")
                .await;
            continue;
        }
        state.store.remove_group_member(*id, user_email).await?;
        let _ = state
            .store
            .log_auth_event("demote", user_email, iss, sub, &format!("removed:{name}"))
            .await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_identity, VerifiedIdentity, SSO_STATE_TTL_SECS};
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
        /// access_token value served at /token (tests smuggle claims here to
        /// prove the verifier never consults it — D-15).
        access_token: Arc<Mutex<String>>,
        /// When enabled, /token refuses a code it already exchanged — real
        /// IdPs enforce single-use authorization codes (plan 17-05 matrix).
        single_use: Arc<Mutex<bool>>,
        used_codes: Arc<Mutex<std::collections::HashSet<String>>>,
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
                access_token: Arc::new(Mutex::new("opaque".to_string())),
                single_use: Arc::new(Mutex::new(false)),
                used_codes: Arc::new(Mutex::new(std::collections::HashSet::new())),
            };

            let (
                claims,
                challenge,
                last_verifier,
                sign_key,
                jwks,
                access_token,
                single_use,
                used_codes,
            ) = (
                idp.claims.clone(),
                idp.challenge.clone(),
                idp.last_verifier.clone(),
                idp.sign_key.clone(),
                idp.jwks_json.clone(),
                idp.access_token.clone(),
                idp.single_use.clone(),
                idp.used_codes.clone(),
            );
            tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let (
                        claims,
                        challenge,
                        last_verifier,
                        sign_key,
                        jwks,
                        access_token,
                        single_use,
                        used_codes,
                    ) = (
                        claims.clone(),
                        challenge.clone(),
                        last_verifier.clone(),
                        sign_key.clone(),
                        jwks.clone(),
                        access_token.clone(),
                        single_use.clone(),
                        used_codes.clone(),
                    );
                    tokio::spawn(async move {
                        let _ = serve(
                            &mut stream,
                            jwks,
                            claims,
                            challenge,
                            last_verifier,
                            sign_key,
                            access_token,
                            single_use,
                            used_codes,
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

        fn set_access_token(&self, token: &str) {
            *self.access_token.lock().unwrap() = token.to_string();
        }

        fn last_verifier(&self) -> Option<String> {
            self.last_verifier.lock().unwrap().clone()
        }

        /// Sign future ID tokens with the foreign key (JWKS still advertises the real one).
        fn sign_with_foreign_key(&self) {
            *self.sign_key.lock().unwrap() = WRONG_KEY_PEM;
        }

        /// Make /token refuse a code it has already exchanged. Real IdPs
        /// enforce single-use authorization codes; the matrix replay test
        /// opts in so a replayed callback URL hits the IdP-side refusal.
        fn enforce_code_single_use(&self) {
            *self.single_use.lock().unwrap() = true;
        }
    }

    async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
        let resp = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes()).await;
    }

    // Test-only mock IdP dispatcher: the Arc<Mutex<..>> handles are one per mock
    // knob; bundling them would obscure the mock more than the flat list helps.
    #[allow(clippy::too_many_arguments)]
    async fn serve(
        stream: &mut TcpStream,
        jwks_json: String,
        claims: Arc<Mutex<Value>>,
        challenge: Arc<Mutex<Option<String>>>,
        last_verifier: Arc<Mutex<Option<String>>>,
        sign_key: Arc<Mutex<&'static str>>,
        access_token: Arc<Mutex<String>>,
        single_use: Arc<Mutex<bool>>,
        used_codes: Arc<Mutex<std::collections::HashSet<String>>>,
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
                // Single-use code gate (opt-in): mirrors real IdPs refusing a
                // replayed authorization code even with a valid PKCE verifier.
                if let Some(code) = get("code") {
                    if *single_use.lock().unwrap() && !used_codes.lock().unwrap().insert(code) {
                        respond(
                            stream,
                            "400 Bad Request",
                            r#"{"error":"invalid_grant","error_description":"code_replay"}"#,
                        )
                        .await;
                        return Ok(());
                    }
                }
                let id_token =
                    sign_rs256(*sign_key.lock().unwrap(), &claims.lock().unwrap().clone());
                let tok = json!({
                    "access_token": *access_token.lock().unwrap(),
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
        // NO link_identity row, and the token carries no email_verified claim —
        // wave-3 resolution fails closed at the email_verified strictness gate.
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
            .count_auth_events("deny", "email_not_verified")
            .await
            .unwrap();
        assert_eq!(
            denies, 1,
            "deny event logged with detail email_not_verified"
        );
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

    // ---------- Identity resolution: link / provision / refuse (plan 17-03 task 2) ----------

    /// Full callback with ID-token claims carrying `email` / `email_verified`.
    async fn callback_with_email(
        app: &axum::Router,
        state: &crate::server::SharedState,
        idp: &MockIdp,
        sub: &str,
        email: Option<&str>,
        email_verified: Option<Value>,
    ) -> Response {
        let (loc, cookie_value) = drive_sso_login(app).await;
        let (s, n, _v) = happy_setup(idp, state, &loc, &cookie_value, sub).await;
        let now = crate::server::unix_now();
        let mut claims = json!({
            "iss": idp.issuer, "aud": [TEST_CLIENT_ID], "sub": sub,
            "nonce": n, "iat": now, "exp": now + 300,
        });
        if let Some(e) = email {
            claims["email"] = json!(e);
        }
        if let Some(v) = email_verified {
            claims["email_verified"] = v;
        }
        idp.set_claims(claims);
        callback(
            app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await
    }

    /// The local email the minted session cookie carries, if any.
    fn session_email(state: &crate::server::SharedState, res: &Response) -> Option<String> {
        let c = find_cookie(&set_cookie_headers(res), "morinet_session")?;
        let token = c
            .split(';')
            .next()
            .unwrap()
            .trim_start_matches("morinet_session=");
        state
            .codec
            .verify(token, crate::server::unix_now())
            .map(|cl| cl.email)
    }

    /// A1 core: returning users are keyed STRICTLY on (iss, sub) — the email
    /// claim is never consulted, so an IdP account carrying someone else's
    /// email cannot steer the login (D-03).
    #[tokio::test]
    async fn sso_returning_user_keyed_on_iss_sub_not_email() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "sso-user-1",
            Some("victim@elsewhere.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            session_email(&state, &res).as_deref(),
            Some("sso-user@example.com"),
            "session carries the LINKED account, never the claim email"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("link", "auto_link")
                .await
                .unwrap(),
            0,
            "returning path never logs a link event"
        );
    }

    /// First login with a verified email matching an existing reader: auto-link,
    /// sign in, log the link (D-12/D-05).
    #[tokio::test]
    async fn sso_auto_links_verified_email_match() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        state
            .store
            .create_user("reader1@example.com", "pw", "reader")
            .await
            .unwrap();
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "brand-new-sub",
            Some("reader1@example.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            session_email(&state, &res).as_deref(),
            Some("reader1@example.com")
        );
        let uid = state
            .store
            .user_id_for("reader1@example.com")
            .await
            .unwrap()
            .unwrap();
        let ident = state.store.identity_for_user(uid).await.unwrap();
        assert_eq!(
            ident,
            Some((
                "oidc".to_string(),
                idp.issuer.clone(),
                "brand-new-sub".to_string()
            )),
            "user_identities row created"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("link", "auto_link")
                .await
                .unwrap(),
            1,
            "every link event is logged (D-05)"
        );
    }

    /// Auto-link requires the JSON boolean `email_verified == true`: false,
    /// missing, or a string "true" all refuse with NO link row.
    #[tokio::test]
    async fn sso_link_refused_when_email_not_verified() {
        // false / missing → deny detail email_not_verified.
        for (label, verified) in [("false", Some(json!(false))), ("missing", None)] {
            let idp = MockIdp::start().await;
            let (app, state) = test_router_with_oidc(&idp.issuer).await;
            state
                .store
                .create_user("reader2@example.com", "pw", "reader")
                .await
                .unwrap();
            let res = callback_with_email(
                &app,
                &state,
                &idp,
                "unverified-sub",
                Some("reader2@example.com"),
                verified,
            )
            .await;
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "{label} must refuse");
            assert!(
                session_email(&state, &res).is_none(),
                "{label} must not sign in"
            );
            let uid = state
                .store
                .user_id_for("reader2@example.com")
                .await
                .unwrap()
                .unwrap();
            assert!(
                state.store.identity_for_user(uid).await.unwrap().is_none(),
                "{label}: no link row"
            );
            assert_eq!(
                state
                    .store
                    .count_auth_events("deny", "email_not_verified")
                    .await
                    .unwrap(),
                1,
                "{label}: deny logged"
            );
        }
        // String "true" is not the JSON boolean true: the strict check refuses.
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        state
            .store
            .create_user("reader3@example.com", "pw", "reader")
            .await
            .unwrap();
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "string-true-sub",
            Some("reader3@example.com"),
            Some(json!("true")),
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "string \"true\" must refuse"
        );
        assert!(session_email(&state, &res).is_none());
        let uid = state
            .store
            .user_id_for("reader3@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(
            state.store.identity_for_user(uid).await.unwrap().is_none(),
            "string \"true\": no link row"
        );
        assert!(
            state
                .store
                .count_auth_events("deny", "email_not_verified")
                .await
                .unwrap()
                + state
                    .store
                    .count_auth_events("deny", "token_invalid")
                    .await
                    .unwrap()
                >= 1,
            "string \"true\": fail-closed deny logged (email_not_verified, or token_invalid \
             when the library rejects the malformed claim before resolution)"
        );
    }

    /// Admin accounts are NEVER auto-linked (D-05 / A1): refuse, no link, no
    /// sign-in, deny logged with the matched account as actor.
    #[tokio::test]
    async fn sso_admin_match_refused() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        // admin@example.com exists via ensure_admin in the harness.
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "admin-sub",
            Some("admin@example.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(session_email(&state, &res).is_none(), "no sign-in");
        let uid = state
            .store
            .user_id_for("admin@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(
            state.store.identity_for_user(uid).await.unwrap().is_none(),
            "admin never auto-linked"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "admin_match")
                .await
                .unwrap(),
            1,
            "refusal logged"
        );
    }

    /// Allowed-domain first-timer: provisioned as reader, linked, logged,
    /// signed in with the lowercase stored email (D-04, D-12).
    #[tokio::test]
    async fn sso_auto_provisions_reader_for_allowed_domain() {
        let idp = MockIdp::start().await;
        let mut cfg = oidc_cfg(&idp.issuer);
        cfg.allowed_email_domains = vec!["example.com".to_string()];
        let (app, state) = test_router_with_cfg(Some(cfg)).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "newbie-sub",
            Some("Newbie@Example.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            session_email(&state, &res).as_deref(),
            Some("newbie@example.com"),
            "provisioned email stored/signed lowercase"
        );
        let (uid, _email, role) = state
            .store
            .user_by_email_normalized("newbie@example.com")
            .await
            .unwrap()
            .expect("provisioned user row");
        assert_eq!(role, "reader", "provisioning is reader-only (D-04)");
        let ident = state.store.identity_for_user(uid).await.unwrap();
        assert_eq!(
            ident.map(|(_, iss, sub)| (iss, sub)),
            Some((idp.issuer.clone(), "newbie-sub".to_string()))
        );
        assert_eq!(
            state
                .store
                .count_auth_events("provision", "auto_provision")
                .await
                .unwrap(),
            1,
            "provision event logged"
        );
    }

    /// The provisioned account's placeholder password is unguessable: password
    /// login with the IdP email + any password fails (T-17-13, criterion 7).
    #[tokio::test]
    async fn sso_provisioned_account_password_login_fails() {
        let idp = MockIdp::start().await;
        let mut cfg = oidc_cfg(&idp.issuer);
        cfg.allowed_email_domains = vec!["example.com".to_string()];
        let (app, state) = test_router_with_cfg(Some(cfg)).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "pwfail-sub",
            Some("pwfail@example.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER, "provisioned first");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let pw = req(
            app,
            "POST",
            "/login",
            headers,
            Some(Body::from("email=pwfail%40example.com&password=whatever")),
        )
        .await;
        assert_eq!(
            pw.status(),
            StatusCode::UNAUTHORIZED,
            "placeholder unguessable"
        );
    }

    /// A7: suffix-similar domain refuses fail-closed — no user row created.
    #[tokio::test]
    async fn sso_unmatched_domain_denied() {
        let idp = MockIdp::start().await;
        let mut cfg = oidc_cfg(&idp.issuer);
        cfg.allowed_email_domains = vec!["example.com".to_string()];
        let (app, state) = test_router_with_cfg(Some(cfg)).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "evil-sub",
            Some("evil@evilexample.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(session_email(&state, &res).is_none());
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "domain_not_allowed")
                .await
                .unwrap(),
            1
        );
        assert!(
            state
                .store
                .user_by_email_normalized("evil@evilexample.com")
                .await
                .unwrap()
                .is_none(),
            "refusal creates no user row"
        );
    }

    /// Empty allowlist ⇒ auto-provision DISABLED entirely (fail-closed);
    /// linking existing accounts still works (tested elsewhere).
    #[tokio::test]
    async fn sso_empty_allowlist_disables_provisioning() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await; // allowed_email_domains = []
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "stranger-sub",
            Some("stranger@anywhere.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(session_email(&state, &res).is_none());
        assert!(
            state
                .store
                .user_by_email_normalized("stranger@anywhere.com")
                .await
                .unwrap()
                .is_none(),
            "nobody is provisioned with an empty allowlist"
        );
    }

    /// D-12: case-variant IdP email links the existing mixed-case account —
    /// one row, no duplicate (T-17-15).
    #[tokio::test]
    async fn sso_email_match_case_insensitive_link() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        state
            .store
            .create_user("User@X.com", "pw", "reader")
            .await
            .unwrap();
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "case-sub",
            Some("user@x.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(session_email(&state, &res).as_deref(), Some("User@X.com"));
        let uid = state
            .store
            .user_id_for("User@X.com")
            .await
            .unwrap()
            .unwrap();
        assert!(
            state.store.identity_for_user(uid).await.unwrap().is_some(),
            "linked to the existing row"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("provision", "auto_provision")
                .await
                .unwrap(),
            0,
            "linked, not provisioned — no duplicate account"
        );
    }

    /// Link race (T-17-14): when the (iss,sub) row already exists — a
    /// concurrent first-login won the INSERT OR IGNORE race — resolution
    /// proceeds as the returning user, keyed on the identity row.
    #[tokio::test]
    async fn sso_link_race_falls_back_to_returning() {
        let (app, state) = test_router_with_oidc("https://issuer.test").await;
        seed_linked_user(
            &state,
            "race-user@example.com",
            "https://issuer.test",
            "race-sub",
        )
        .await;
        // A DIFFERENT account matches the claim email — the (iss,sub) hit must win.
        state
            .store
            .create_user("other@example.com", "pw", "reader")
            .await
            .unwrap();
        let oidc = oidc_cfg("https://issuer.test");
        let claims = VerifiedIdentity {
            sub: "race-sub".to_string(),
            email: Some("other@example.com".to_string()),
            email_verified: Some(true),
        };
        let email = resolve_identity(&state, &oidc, "https://issuer.test", &claims)
            .await
            .expect("returns as the linked user");
        assert_eq!(email, "race-user@example.com");
        drop(app);
    }

    // ---------- Account page "Linked via SSO" indicator (plan 17-03 task 3, D-13) ----------

    /// Password-login cookie for the harness admin (same shape as server.rs tests).
    async fn account_login_cookie(app: &axum::Router) -> String {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/login",
            headers,
            Some(Body::from(
                "email=admin%40example.com&password=test-pass-9f3a",
            )),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        res.headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn account_page_shows_sso_identity_row() {
        let (app, state) = test_router_disabled().await;
        let cookie = account_login_cookie(&app).await;
        let uid = state
            .store
            .user_id_for("admin@example.com")
            .await
            .unwrap()
            .unwrap();
        state
            .store
            .link_identity("oidc", "https://idp.example/", "sub-9", uid)
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, cookie.parse().unwrap());
        let res = req(app, "GET", "/account", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("Linked via SSO (https://idp.example/)"),
            "SSO-linked account shows the indicator row: {body}"
        );
    }

    #[tokio::test]
    async fn account_page_no_sso_row_for_password_users() {
        let (app, _) = test_router_disabled().await;
        let cookie = account_login_cookie(&app).await;
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, cookie.parse().unwrap());
        let res = req(app, "GET", "/account", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            !body.contains("Linked via SSO"),
            "password-only account must be byte-identical to today: {body}"
        );
    }

    #[tokio::test]
    async fn account_page_sso_row_survives_token_create_render() {
        let (app, state) = test_router_disabled().await;
        let cookie = account_login_cookie(&app).await;
        let uid = state
            .store
            .user_id_for("admin@example.com")
            .await
            .unwrap()
            .unwrap();
        state
            .store
            .link_identity("oidc", "https://idp.example/", "sub-9", uid)
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, cookie.parse().unwrap());
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app,
            "POST",
            "/account/tokens",
            headers,
            Some(Body::from("name=laptop")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("Linked via SSO (https://idp.example/)"),
            "the create_account_token re-render keeps the row: {body}"
        );
    }

    // ---------- Group sync: replace-on-login (plan 17-04, D-09/D-10/D-15) ----------

    /// `oidc_cfg` with a non-empty claim->group mapping (managed set = values).
    fn oidc_cfg_mapped(issuer: &str, map: &[(&str, &str)]) -> crate::config::OidcRuntime {
        let mut cfg = oidc_cfg(issuer);
        cfg.group_map = map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        cfg
    }

    /// Full callback whose ID-token claims carry an optional `groups` claim
    /// (`None` = claim absent entirely).
    async fn callback_with_groups(
        app: &axum::Router,
        state: &crate::server::SharedState,
        idp: &MockIdp,
        sub: &str,
        groups: Option<Value>,
    ) -> Response {
        let (loc, cookie_value) = drive_sso_login(app).await;
        let (s, n, _v) = happy_setup(idp, state, &loc, &cookie_value, sub).await;
        let now = crate::server::unix_now();
        let mut claims = json!({
            "iss": idp.issuer, "aud": [TEST_CLIENT_ID], "sub": sub,
            "nonce": n, "iat": now, "exp": now + 300,
        });
        if let Some(g) = groups {
            claims["groups"] = g;
        }
        idp.set_claims(claims);
        callback(
            app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await
    }

    /// Linked SSO reader used by every group_sync test.
    async fn seed_sync_user(state: &crate::server::SharedState, idp: &MockIdp) {
        seed_linked_user(state, "sync@example.com", &idp.issuer, "sync-sub").await;
    }

    /// D-09: claim ABSENT -> managed memberships untouched, no audit rows.
    #[tokio::test]
    async fn group_sync_claim_absent_keeps_memberships() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        state
            .store
            .add_group_member(writers, "sync@example.com")
            .await
            .unwrap();
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(writers).await.unwrap(),
            vec!["sync@example.com"],
            "absent claim must never touch managed memberships (D-09)"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("group_sync", "added:writers")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            state
                .store
                .count_auth_events("demote", "removed:writers")
                .await
                .unwrap(),
            0
        );
    }

    /// D-09: claim PRESENT but empty -> ALL managed memberships removed, and
    /// every removal is audited as a `demote` row (T-17-18).
    #[tokio::test]
    async fn group_sync_claim_present_empty_removes_managed() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        state
            .store
            .add_group_member(writers, "sync@example.com")
            .await
            .unwrap();
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!([]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(state.store.group_members(writers).await.unwrap().is_empty());
        assert_eq!(
            state
                .store
                .count_auth_events("demote", "removed:writers")
                .await
                .unwrap(),
            1,
            "demotions are logged (D-09)"
        );
    }

    /// Mapped claim value -> membership added + `group_sync` audit row.
    #[tokio::test]
    async fn group_sync_adds_mapped_groups() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!(["dev"]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(writers).await.unwrap(),
            vec!["sync@example.com"]
        );
        assert_eq!(
            state
                .store
                .count_auth_events("group_sync", "added:writers")
                .await
                .unwrap(),
            1
        );
    }

    /// D-10: groups never named as mapping targets are NEVER touched —
    /// manually-granted memberships survive an emptied claim.
    #[tokio::test]
    async fn group_sync_never_touches_unmanaged_groups() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let manual = state.store.create_group("manual").await.unwrap();
        state
            .store
            .add_group_member(manual, "sync@example.com")
            .await
            .unwrap();
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!([]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(manual).await.unwrap(),
            vec!["sync@example.com"],
            "unmanaged group survives sync (D-10)"
        );
    }

    /// A5 core: with NO mapping value naming `admin`, a claim value can never
    /// add admin membership — the group is not even in the managed set.
    #[tokio::test]
    async fn group_sync_admin_mapping_absent_blocks_admin() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let admin = state.store.create_group("admin").await.unwrap();
        let res =
            callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!(["admins"]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            state.store.group_members(admin).await.unwrap().is_empty(),
            "claim cannot self-serve admin without an explicit mapping (T-17-16)"
        );
    }

    /// Admin mapping IS opt-in: configured mapping adds admin membership;
    /// a later login with the claim ABSENT keeps it (absent != empty, D-09).
    #[tokio::test]
    async fn group_sync_admin_mapping_present_adds_and_absent_keeps() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("admins", "admin")]))).await;
        seed_sync_user(&state, &idp).await;
        let admin = state.store.create_group("admin").await.unwrap();
        let res =
            callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!(["admins"]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(admin).await.unwrap(),
            vec!["sync@example.com"],
            "explicit mapping grants admin (opt-in)"
        );
        // Second login, claim absent: admin membership must be KEPT, never
        // silently stripped (D-09/D-10).
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(admin).await.unwrap(),
            vec!["sync@example.com"],
            "absent claim never demotes (D-09)"
        );
    }

    /// Anti-lockout (D-07 spirit): removing the LAST admin via sync is
    /// refused and logged `last_admin_kept`.
    #[tokio::test]
    async fn group_sync_last_admin_kept() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "admin")]))).await;
        seed_sync_user(&state, &idp).await;
        let admin = state.store.create_group("admin").await.unwrap();
        state
            .store
            .add_group_member(admin, "sync@example.com")
            .await
            .unwrap();
        // Sole member of `admin`, claim empty -> removal would lock the instance out.
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!([]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(admin).await.unwrap(),
            vec!["sync@example.com"],
            "last admin is never removed (T-17-19)"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "last_admin_kept")
                .await
                .unwrap(),
            1
        );
    }

    /// Wrong-shape claim (string, not array) -> skip sync entirely + deny
    /// `groups_claim_invalid` (fail-closed, T-17-16).
    #[tokio::test]
    async fn group_sync_claim_wrong_shape_skips_and_logs() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!("dev"))).await;
        assert_eq!(
            res.status(),
            StatusCode::SEE_OTHER,
            "sign-in still succeeds"
        );
        assert!(
            state.store.group_members(writers).await.unwrap().is_empty(),
            "garbage claim never acts on memberships"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "groups_claim_invalid")
                .await
                .unwrap(),
            1
        );
    }

    /// Unknown mapping target: that entry is skipped + deny
    /// `unknown_group_target`; other entries still apply; no group is created.
    #[tokio::test]
    async fn group_sync_unknown_target_group_skipped() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_cfg(Some(oidc_cfg_mapped(
            &idp.issuer,
            &[("dev", "ghost"), ("dev2", "writers")],
        )))
        .await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        let res =
            callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!(["dev", "dev2"])))
                .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(writers).await.unwrap(),
            vec!["sync@example.com"],
            "valid entries still apply"
        );
        assert_eq!(state.store.group_id_for_name("ghost").await.unwrap(), None);
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "unknown_group_target")
                .await
                .unwrap(),
            1
        );
    }

    /// Non-string array entries are ignored; string entries still apply.
    #[tokio::test]
    async fn group_sync_nonstring_entries_ignored() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        let res =
            callback_with_groups(&app, &state, &idp, "sync-sub", Some(json!(["dev", 42]))).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state.store.group_members(writers).await.unwrap(),
            vec!["sync@example.com"],
            "42 ignored, dev applied"
        );
    }

    /// D-15: groups are read ONLY from the signature-verified ID token —
    /// the same claim smuggled into the access token changes nothing.
    #[tokio::test]
    async fn group_sync_reads_only_id_token() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_sync_user(&state, &idp).await;
        let writers = state.store.create_group("writers").await.unwrap();
        // groups live ONLY in the access-token payload (unsigned by policy).
        idp.set_access_token(&sign_rs256(
            TEST_KEY_PEM,
            &json!({ "sub": "sync-sub", "groups": ["dev"] }),
        ));
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            state.store.group_members(writers).await.unwrap().is_empty(),
            "access-token claims are never consulted (D-15)"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("group_sync", "added:writers")
                .await
                .unwrap(),
            0
        );
    }

    // ---------- plan 17-05: end-to-end security matrix (A1/A2/A5/A6/A7) ----------
    //
    // Each family drives the REAL router + mock IdP through one ROADMAP
    // "Avoids" entry. Every refusal asserts BOTH the HTTP status AND its
    // auth_events row (kind + detail) — D-05 ("every link/deny is logged")
    // made mechanically checkable across families.

    /// Bare `name=value` of the minted session cookie, ready for a Cookie header.
    fn minted_session_cookie(res: &Response) -> Option<String> {
        let c = find_cookie(&set_cookie_headers(res), "morinet_session")?;
        Some(c.split(';').next().unwrap().to_string())
    }

    async fn get_with_cookie(app: &axum::Router, uri: &str, cookie: &str) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, cookie.parse().unwrap());
        req(app.clone(), "GET", uri, headers, None).await
    }

    async fn post_login(app: &axum::Router, email: &str, password: &str) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        req(
            app.clone(),
            "POST",
            "/login",
            headers,
            Some(Body::from(format!("email={email}&password={password}"))),
        )
        .await
    }

    // ----- A1: email-takeover attempts (D-03/D-05/D-12) -----

    /// An IdP account presenting the ADMIN's verified email is refused: no
    /// auto-link, no sign-in, refusal audited (D-05, A1).
    #[tokio::test]
    async fn matrix_a1_admin_never_linked() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "attacker-sub",
            Some("admin@example.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(
            find_cookie(&set_cookie_headers(&res), "morinet_session").is_none(),
            "refusal must never carry a session cookie"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "admin_match")
                .await
                .unwrap(),
            1,
            "admin-match refusal is audited"
        );
        let uid = state
            .store
            .user_id_for("admin@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(
            state.store.identity_for_user(uid).await.unwrap().is_none(),
            "admin account gained no SSO identity"
        );
    }

    /// After the first link the email claim is INERT: sub-A stays pinned to the
    /// victim even when the token carries an attacker email (D-03, A1).
    #[tokio::test]
    async fn matrix_a1_relinked_identity_ignores_email() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "victim@corp.example", &idp.issuer, "sub-A").await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "sub-A",
            Some("attacker@evil.test"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            session_email(&state, &res).as_deref(),
            Some("victim@corp.example"),
            "session follows the (iss,sub) link, never the claim email"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("link", "auto_link")
                .await
                .unwrap(),
            0,
            "returning path never re-links on email"
        );
        assert!(
            state
                .store
                .user_id_for("attacker@evil.test")
                .await
                .unwrap()
                .is_none(),
            "the attacker email must not gain an account"
        );
    }

    /// The IdP changing a user's email never moves the identity: same sub →
    /// same local user, no new account, no re-link audit (D-03, A1).
    #[tokio::test]
    async fn matrix_a1_email_change_at_idp_does_not_move_identity() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "user-a@corp.example", &idp.issuer, "sub-A").await;
        let first = callback_with_email(
            &app,
            &state,
            &idp,
            "sub-A",
            Some("user-a@corp.example"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(
            session_email(&state, &first).as_deref(),
            Some("user-a@corp.example")
        );
        // Later login: same sub, DIFFERENT email claim.
        let later = callback_with_email(
            &app,
            &state,
            &idp,
            "sub-A",
            Some("someone-else@nowhere.test"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(later.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            session_email(&state, &later).as_deref(),
            Some("user-a@corp.example"),
            "identity stays pinned to the linked user"
        );
        assert!(
            state
                .store
                .user_id_for("someone-else@nowhere.test")
                .await
                .unwrap()
                .is_none(),
            "a changed claim email must not provision a new account"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("link", "auto_link")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            state
                .store
                .count_auth_events("provision", "auto_provision")
                .await
                .unwrap(),
            0
        );
    }

    // ----- A2: state / nonce / PKCE / code replay -----

    /// Callback without the state cookie: 403. Pre-parse denies are NOT audited
    /// (REV-17-01: unauthenticated request-per-row flood on auth_events).
    #[tokio::test]
    async fn matrix_a2_missing_state() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        let res = callback(&app, "code=test-code&state=whatever", None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "state_invalid")
                .await
                .unwrap(),
            0,
            "pre-parse denies are not audited (REV-17-01)"
        );
    }

    /// State cookie signed with a foreign secret: codec rejects it, audited.
    #[tokio::test]
    async fn matrix_a2_forged_state() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        let (_loc, cookie_value) = drive_sso_login(&app).await;
        let (s, n, v) = decode_state_cookie(&state, &cookie_value);
        let rogue = crate::session::SessionCodec::new(b"attacker-secret-0123456789ab");
        let forged = rogue.sign(
            &format!("{s}\n{n}\n{v}"),
            crate::server::unix_now(),
            SSO_STATE_TTL_SECS,
        );
        let res = callback(&app, &format!("code=test-code&state={s}"), Some(&forged)).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "state_invalid")
                .await
                .unwrap(),
            0,
            "pre-parse denies are not audited (REV-17-01)"
        );
    }

    /// State cookie past its 600 s TTL (signed with a far-past `now`): the
    /// codec expiry rejects it; pre-parse deny, not audited (REV-17-01).
    #[tokio::test]
    async fn matrix_a2_expired_state() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        let (_loc, cookie_value) = drive_sso_login(&app).await;
        let (s, n, v) = decode_state_cookie(&state, &cookie_value);
        let stale = state.codec.sign(
            &format!("{s}\n{n}\n{v}"),
            crate::server::unix_now() - (SSO_STATE_TTL_SECS + 60),
            SSO_STATE_TTL_SECS,
        );
        let res = callback(&app, &format!("code=test-code&state={s}"), Some(&stale)).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "state_invalid")
                .await
                .unwrap(),
            0,
            "pre-parse denies are not audited (REV-17-01)"
        );
    }

    /// ID token carrying the wrong nonce: exchange fails closed, audited.
    #[tokio::test]
    async fn matrix_a2_nonce_mismatch() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
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
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "nonce_mismatch")
                .await
                .unwrap(),
            1
        );
    }

    /// Same callback URL replayed twice (attacker ignores the state-cookie
    /// clear): the IdP's single-use code gate refuses the second exchange —
    /// 403, audited `token_invalid`, no session on the replay.
    #[tokio::test]
    async fn matrix_a2_code_replay() {
        let idp = MockIdp::start().await;
        idp.enforce_code_single_use();
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        let query = format!("code=test-code&state={s}");
        let first = callback(&app, &query, Some(&cookie_value)).await;
        assert_eq!(first.status(), StatusCode::SEE_OTHER);
        let replay = callback(&app, &query, Some(&cookie_value)).await;
        assert_eq!(replay.status(), StatusCode::FORBIDDEN);
        assert!(
            find_cookie(&set_cookie_headers(&replay), "morinet_session").is_none(),
            "replay must not mint a session"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "token_invalid")
                .await
                .unwrap(),
            1
        );
    }

    /// PKCE gate: the verifier must hash to the challenge the IdP stored —
    /// a mismatched challenge kills the exchange, audited `token_invalid`.
    #[tokio::test]
    async fn matrix_a2_pkce_verifier_required() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, v) = happy_setup(&idp, &state, &loc, &cookie_value, "sso-user-1").await;
        // IdP now expects a DIFFERENT challenge: our verifier no longer matches.
        idp.set_challenge("bogus-challenge-not-matching-verifier");
        let res = callback(
            &app,
            &format!("code=test-code&state={s}"),
            Some(&cookie_value),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(find_cookie(&set_cookie_headers(&res), "morinet_session").is_none());
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "token_invalid")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            idp.last_verifier().as_deref(),
            Some(v.as_str()),
            "the verifier reached the IdP and was checked"
        );
    }

    // ----- A5: groups-claim escalation (D-09/D-10/D-15) -----

    /// A groups claim naming "admin" with NO mapping entry can never grant
    /// admin: the group is not in the managed set, and the /admin gate stays
    /// shut for the signed-in reader (T-17-16).
    #[tokio::test]
    async fn matrix_a5_no_admin_claim_without_mapping() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_linked_user(&state, "sync@example.com", &idp.issuer, "sync-sub").await;
        let admin = state.store.create_group("admin").await.unwrap();
        let res = callback_with_groups(
            &app,
            &state,
            &idp,
            "sync-sub",
            Some(json!(["admins", "admin"])),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            state.store.group_members(admin).await.unwrap().is_empty(),
            "claim cannot self-serve admin without an explicit mapping"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("group_sync", "added:admin")
                .await
                .unwrap(),
            0
        );
        let cookie = minted_session_cookie(&res).expect("session minted");
        let gate = get_with_cookie(&app, "/admin", &cookie).await;
        assert_eq!(gate.status(), StatusCode::FORBIDDEN, "role stays reader");
    }

    /// The same claim smuggled into the ACCESS token changes nothing: only the
    /// signature-verified ID token is consulted (D-15).
    #[tokio::test]
    async fn matrix_a5_access_token_claim_ignored() {
        let idp = MockIdp::start().await;
        let (app, state) =
            test_router_with_cfg(Some(oidc_cfg_mapped(&idp.issuer, &[("dev", "writers")]))).await;
        seed_linked_user(&state, "sync@example.com", &idp.issuer, "sync-sub").await;
        let writers = state.store.create_group("writers").await.unwrap();
        idp.set_access_token(&sign_rs256(
            TEST_KEY_PEM,
            &json!({ "sub": "sync-sub", "groups": ["dev"] }),
        ));
        let res = callback_with_groups(&app, &state, &idp, "sync-sub", None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            state.store.group_members(writers).await.unwrap().is_empty(),
            "access-token claims are never consulted"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("group_sync", "added:writers")
                .await
                .unwrap(),
            0
        );
    }

    /// ID token signed with a foreign key (JWKS advertises the real one):
    /// verification fails closed, audited `token_invalid`.
    #[tokio::test]
    async fn matrix_a5_forged_key_rejected() {
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
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "token_invalid")
                .await
                .unwrap(),
            1
        );
    }

    // ----- A6: session fixation -----

    /// A hostile pre-set `morinet_session` cookie is REPLACED by the fresh
    /// mint on callback success, and never authenticates on its own (T-17-22).
    #[tokio::test]
    async fn matrix_a6_fresh_cookie_on_sso_login() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "victim@corp.example", &idp.issuer, "sub-A").await;
        let (loc, cookie_value) = drive_sso_login(&app).await;
        let (s, _n, _v) = happy_setup(&idp, &state, &loc, &cookie_value, "sub-A").await;
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("morinet_session=attacker-forged-token; morinet_sso_state={cookie_value}")
                .parse()
                .unwrap(),
        );
        let res = req(
            app.clone(),
            "GET",
            &format!("/sso/callback?code=test-code&state={s}"),
            headers,
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            session_email(&state, &res).as_deref(),
            Some("victim@corp.example"),
            "callback mints a FRESH session replacing any pre-set cookie"
        );
        // The hostile cookie alone never authenticates.
        let hostile = get_with_cookie(&app, "/", "morinet_session=attacker-forged-token").await;
        assert_eq!(hostile.status(), StatusCode::SEE_OTHER);
        assert_eq!(hostile.headers().get(LOCATION).unwrap(), "/login");
    }

    /// Nothing handed out by /sso/login is accepted as a session identifier:
    /// the state token shares the session codec, so it must NEVER verify as a
    /// session cookie (A6 — this test drove the `verify_session` fix).
    #[tokio::test]
    async fn matrix_a6_no_preauth_identifier() {
        let idp = MockIdp::start().await;
        let (app, _state) = test_router_with_oidc(&idp.issuer).await;
        let (_loc, state_value) = drive_sso_login(&app).await;
        let res = get_with_cookie(&app, "/", &format!("morinet_session={state_value}")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            res.headers().get(LOCATION).unwrap(),
            "/login",
            "the SSO state token must not authenticate a session"
        );
        let gate = get_with_cookie(&app, "/admin", &format!("morinet_session={state_value}")).await;
        assert_eq!(gate.status(), StatusCode::FORBIDDEN);
    }

    // ----- A7: gate bypass via auto-provision (D-04) -----

    /// An auto-provisioned SSO reader hits the SAME gates as any reader:
    /// / allowed, /admin refused; provisioning is audited.
    #[tokio::test]
    async fn matrix_a7_provisioned_reader_blocked_from_admin() {
        let idp = MockIdp::start().await;
        let mut cfg = oidc_cfg(&idp.issuer);
        cfg.allowed_email_domains = vec!["example.com".to_string()];
        let (app, state) = test_router_with_cfg(Some(cfg)).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "fresh-sub",
            Some("fresh@example.com"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state
                .store
                .count_auth_events("provision", "auto_provision")
                .await
                .unwrap(),
            1
        );
        let cookie = minted_session_cookie(&res).expect("session minted");
        let home = get_with_cookie(&app, "/", &cookie).await;
        assert_eq!(home.status(), StatusCode::OK);
        let admin = get_with_cookie(&app, "/admin", &cookie).await;
        assert_eq!(
            admin.status(),
            StatusCode::FORBIDDEN,
            "provisioned reader never reaches admin"
        );
    }

    /// Unmatched domain: 403 with NO session cookie anywhere in the response,
    /// audited `domain_not_allowed` (fail-closed provisioning).
    #[tokio::test]
    async fn matrix_a7_unmatched_email_gets_no_cookie() {
        let idp = MockIdp::start().await;
        let mut cfg = oidc_cfg(&idp.issuer);
        cfg.allowed_email_domains = vec!["example.com".to_string()];
        let (app, state) = test_router_with_cfg(Some(cfg)).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "x-sub",
            Some("nope@evil.test"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(
            set_cookie_headers(&res)
                .iter()
                .all(|c| !c.starts_with("morinet_session=")),
            "403 must not leak a session cookie: {:?}",
            set_cookie_headers(&res)
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "domain_not_allowed")
                .await
                .unwrap(),
            1
        );
    }

    /// With `[oidc]` absent both SSO routes answer 404 and the login page is
    /// byte-identical to pre-Phase-17 LOGIN_HTML — SSO is fully removable
    /// (SSO-05).
    #[tokio::test]
    async fn matrix_a7_sso_disabled_routes_404() {
        let (app, _state) = test_router_disabled().await;
        let login = req(app.clone(), "GET", "/sso/login", HeaderMap::new(), None).await;
        assert_eq!(login.status(), StatusCode::NOT_FOUND);
        let cb = req(
            app.clone(),
            "GET",
            "/sso/callback?code=a&state=b",
            HeaderMap::new(),
            None,
        )
        .await;
        assert_eq!(cb.status(), StatusCode::NOT_FOUND);
        let page = req(app, "GET", "/login", HeaderMap::new(), None).await;
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(
            body_text(page).await,
            crate::server::LOGIN_HTML,
            "login page byte-identical when SSO is absent"
        );
    }

    // ----- Coexistence (SSO-06) -----

    /// REV-17-01: unauthenticated callback flood must not write auth_events
    /// rows (pre-parse denies are dropped; the 403 + cookie clear remain).
    #[tokio::test]
    async fn matrix_a2_deny_flood_not_audited() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        for i in 0..90 {
            let res = callback(&app, &format!("code=flood-{i}&state=nope"), None).await;
            assert_eq!(res.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "state_invalid")
                .await
                .unwrap(),
            0,
            "flood of 90 unauthenticated callbacks must insert zero audit rows"
        );
    }

    /// REV-17-03: an email claim carrying control characters passes the domain
    /// allowlist but would provision an account whose minted session
    /// `verify_session` then rejects — refuse before provisioning instead.
    #[tokio::test]
    async fn sso_email_control_chars_refused() {
        let idp = MockIdp::start().await;
        let mut cfg = oidc_cfg(&idp.issuer);
        cfg.allowed_email_domains = vec!["corp.example".to_string()];
        let (app, state) = test_router_with_cfg(Some(cfg)).await;
        let res = callback_with_email(
            &app,
            &state,
            &idp,
            "ctrl-sub",
            Some("evil\n@corp.example"),
            Some(json!(true)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(
            find_cookie(&set_cookie_headers(&res), "morinet_session").is_none(),
            "no session for a control-char email"
        );
        assert!(
            state
                .store
                .user_by_email_normalized("evil\n@corp.example")
                .await
                .unwrap()
                .is_none(),
            "no account provisioned from a control-char email"
        );
        assert_eq!(
            state
                .store
                .count_auth_events("deny", "invalid_email")
                .await
                .unwrap(),
            1,
            "refusal audited post-exchange (real IdP session behind it)"
        );
    }

    /// With `[oidc]` fully configured, password login for admin AND reader AND
    /// the bootstrap fallback admin still works end-to-end, and an SSO reader
    /// hits the same gates as a password reader — SSO coexists, never replaces.
    #[tokio::test]
    async fn matrix_coexist_password_and_sso_side_by_side() {
        let idp = MockIdp::start().await;
        let (app, state) = test_router_with_oidc(&idp.issuer).await;
        seed_linked_user(&state, "sso-user@example.com", &idp.issuer, "sso-user-1").await;
        // 1. Bootstrap admin password login (SSO-06 safety net).
        let res = post_login(&app, "admin%40example.com", "test-pass-9f3a").await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let admin_cookie = minted_session_cookie(&res).expect("admin session");
        assert_eq!(
            get_with_cookie(&app, "/admin", &admin_cookie)
                .await
                .status(),
            StatusCode::OK
        );
        // 2. Password reader coexists with identical gates.
        state
            .store
            .create_user("pw-reader@example.com", "pw-reader-9f3a", "reader")
            .await
            .unwrap();
        let res = post_login(&app, "pw-reader%40example.com", "pw-reader-9f3a").await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let reader_cookie = minted_session_cookie(&res).expect("reader session");
        assert_eq!(
            get_with_cookie(&app, "/", &reader_cookie).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            get_with_cookie(&app, "/admin", &reader_cookie)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        // 3. SSO login side by side, same gate behaviour as the password reader.
        let res = callback_with_email(&app, &state, &idp, "sso-user-1", None, None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let sso_cookie = minted_session_cookie(&res).expect("sso session");
        assert_eq!(
            get_with_cookie(&app, "/", &sso_cookie).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            get_with_cookie(&app, "/admin", &sso_cookie).await.status(),
            StatusCode::FORBIDDEN
        );
        // 4. Admin password login STILL works after SSO activity.
        assert_eq!(
            post_login(&app, "admin%40example.com", "test-pass-9f3a")
                .await
                .status(),
            StatusCode::SEE_OTHER
        );
    }
}

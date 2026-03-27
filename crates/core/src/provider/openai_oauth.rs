//! OpenAI Codex OAuth 2.0 PKCE flow for ChatGPT subscription authentication.
//!
//! Allows users to sign in with their ChatGPT Plus/Pro account instead of
//! providing a separate API key. Uses the same public client ID as the
//! official Codex CLI.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPES: &str = "openid profile email offline_access";

/// PKCE parameters generated for one OAuth session.
#[derive(Debug, Clone)]
pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

/// Stored OAuth credentials (serialized to config).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp in milliseconds when the access token expires.
    pub expires_at: i64,
}

/// Generate a PKCE code verifier / challenge pair and a random state string.
pub fn generate_pkce() -> PkcePair {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // 32 random bytes → base64url
    let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.r#gen::<u8>()).collect();
    let code_verifier = base64url_encode(&verifier_bytes);

    // SHA-256 of the verifier → base64url
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    let code_challenge = base64url_encode(&digest);

    // Random state
    let state_bytes: Vec<u8> = (0..16).map(|_| rng.r#gen::<u8>()).collect();
    let state = base64url_encode(&state_bytes);

    PkcePair { code_verifier, code_challenge, state }
}

/// Build the full authorization URL the user's browser should open.
pub fn build_authorize_url(pkce: &PkcePair) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true",
        AUTHORIZE_URL,
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(SCOPES),
        urlencoding::encode(&pkce.state),
        urlencoding::encode(&pkce.code_challenge),
    )
}

/// Start a temporary HTTP server on port 1455 that waits for the OAuth
/// callback, extracts the authorization code, and returns it.
///
/// The server serves a single request and shuts down.
pub async fn wait_for_callback() -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:1455")
        .await
        .map_err(|e| format!("Failed to bind port 1455: {}", e))?;

    let (mut stream, _addr) = listener
        .accept()
        .await
        .map_err(|e| format!("Failed to accept connection: {}", e))?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Failed to read request: {}", e))?;

    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the GET request line to extract query params
    // e.g. GET /auth/callback?code=xxx&state=yyy HTTP/1.1
    let code = extract_query_param(&request, "code")
        .ok_or_else(|| "No 'code' parameter in callback".to_string())?;

    // Send a nice response to the browser
    let html = r#"<!DOCTYPE html><html><body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#111;color:#eee"><div style="text-align:center"><h2>Authentication successful!</h2><p>You can close this tab and return to freako.</p></div></body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;

    Ok(code)
}

/// Exchange an authorization code for access + refresh tokens.
pub async fn exchange_code(
    code: &str,
    code_verifier: &str,
) -> Result<OAuthCredentials, String> {
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", CLIENT_ID),
        ("redirect_uri", REDIRECT_URI),
        ("code_verifier", code_verifier),
    ];

    let resp = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", body));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let access_token = body["access_token"]
        .as_str()
        .ok_or("Missing access_token")?
        .to_string();
    let refresh_token = body["refresh_token"]
        .as_str()
        .ok_or("Missing refresh_token")?
        .to_string();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
    let expires_at = chrono::Utc::now().timestamp_millis() + (expires_in * 1000);

    Ok(OAuthCredentials { access_token, refresh_token, expires_at })
}

/// Refresh an expired access token using the refresh token.
pub async fn refresh_token(refresh: &str) -> Result<OAuthCredentials, String> {
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", CLIENT_ID),
    ];

    let resp = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed: {}", body));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse refresh response: {}", e))?;

    let access_token = body["access_token"]
        .as_str()
        .ok_or("Missing access_token")?
        .to_string();
    let new_refresh = body["refresh_token"]
        .as_str()
        .unwrap_or(refresh)
        .to_string();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
    let expires_at = chrono::Utc::now().timestamp_millis() + (expires_in * 1000);

    Ok(OAuthCredentials {
        access_token,
        refresh_token: new_refresh,
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base64url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Extract a query parameter value from a raw HTTP request string.
fn extract_query_param(request: &str, param: &str) -> Option<String> {
    // Find the request line (first line)
    let first_line = request.lines().next()?;
    // e.g. "GET /auth/callback?code=abc&state=xyz HTTP/1.1"
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next()?;
        let value = kv.next()?;
        if key == param {
            return Some(urlencoding::decode(value).unwrap_or_default().to_string());
        }
    }
    None
}

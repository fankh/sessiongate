use std::{collections::HashSet, env, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{ConnectInfo, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use subtle::ConstantTimeEq;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;
use vpn_rdp_portal::{
    auth::{
        hash_password, mfa_satisfied, random_token, token_hash, verify_password, CSRF_BYTES,
        SESSION_BYTES,
    },
    encrypted_json_auth_with_credentials,
    management::{CredentialProvider, CredentialReferenceInput},
    validate_target, RdpCredentials, RdpPolicy, RdpTarget,
};

type ApiError = (StatusCode, String);
type ApiResult<T> = Result<T, ApiError>;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    bearer_token: Arc<str>,
    portal_user: Arc<str>,
    allowed_origin: Arc<str>,
    json_secret: [u8; 16],
    guacamole_base_url: Arc<str>,
}

#[derive(FromRow)]
struct EffectiveAccess {
    id: Uuid,
    name: String,
    hostname: String,
    port: i32,
    domain: String,
    certificate_fingerprint: String,
    clipboard_to_browser: bool,
    clipboard_to_remote: bool,
    upload: bool,
    download: bool,
    printing: bool,
    audio_output: bool,
    microphone: bool,
    recording: bool,
    maximum_duration_seconds: i32,
}

impl EffectiveAccess {
    fn target(&self) -> RdpTarget {
        RdpTarget {
            id: self.id.to_string(),
            name: self.name.clone(),
            hostname: self.hostname.clone(),
            port: self.port as u16,
            certificate_fingerprint: self.certificate_fingerprint.clone(),
            domain: self.domain.clone(),
        }
    }

    fn policy(&self) -> RdpPolicy {
        RdpPolicy {
            clipboard_to_browser: self.clipboard_to_browser,
            clipboard_to_remote: self.clipboard_to_remote,
            upload: self.upload,
            download: self.download,
            printing: self.printing,
            audio_output: self.audio_output,
            microphone: self.microphone,
            recording: self.recording,
            maximum_duration_seconds: self.maximum_duration_seconds as u64,
        }
    }
}

#[derive(Serialize)]
struct PublicTarget {
    id: Uuid,
    name: String,
    policy: RdpPolicy,
}

#[derive(Deserialize)]
struct LaunchRequest {
    target_id: Uuid,
    rdp_username: String,
    rdp_password: String,
}

#[derive(Serialize)]
struct LaunchResponse {
    session_id: Uuid,
    expires_in_seconds: u64,
    guacamole_url: String,
    replay_window_notice: &'static str,
}

#[derive(Deserialize)]
struct TargetInput {
    id: Option<Uuid>,
    name: String,
    hostname: String,
    #[serde(default = "rdp_port")]
    port: u16,
    #[serde(default)]
    domain: String,
    certificate_fingerprint: String,
    network_zone: String,
    #[serde(default)]
    enabled: bool,
}

#[derive(Deserialize)]
struct PolicyInput {
    id: Option<Uuid>,
    name: String,
    #[serde(default)]
    priority: i32,
    #[serde(flatten)]
    policy: RdpPolicy,
    #[serde(default)]
    enabled: bool,
}

#[derive(Deserialize)]
struct BindingInput {
    id: Option<Uuid>,
    policy_id: Uuid,
    target_id: Uuid,
    subject_id: String,
    #[serde(default)]
    priority: i32,
}

#[derive(FromRow, Serialize)]
struct AdminTarget {
    id: Uuid,
    name: String,
    hostname: String,
    port: i32,
    domain: String,
    certificate_fingerprint: String,
    network_zone: String,
    credential_ref: Option<Uuid>,
    enabled: bool,
}

#[derive(FromRow, Serialize)]
struct AdminPolicy {
    id: Uuid,
    name: String,
    priority: i32,
    clipboard_to_browser: bool,
    clipboard_to_remote: bool,
    upload: bool,
    download: bool,
    printing: bool,
    audio_output: bool,
    microphone: bool,
    maximum_duration_seconds: i32,
    enabled: bool,
}

#[derive(FromRow, Serialize)]
struct AdminBinding {
    id: Uuid,
    policy_id: Uuid,
    target_id: Uuid,
    subject_type: String,
    subject_id: String,
    priority: i32,
}

#[derive(FromRow, Serialize)]
struct CredentialReferenceRow {
    id: Uuid,
    name: String,
    provider: String,
    external_ref: String,
    status: String,
    last_rotated_at: Option<String>,
}

#[derive(Deserialize)]
struct CredentialReferenceRequest {
    name: String,
    provider: String,
    external_ref: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
    #[serde(default)]
    totp: String,
}

#[derive(FromRow)]
struct LoginUser {
    id: Uuid,
    username: String,
    password_hash: String,
    totp_secret: Vec<u8>,
    roles: Vec<String>,
    login_allowed: bool,
}

#[derive(Serialize)]
struct LoginResponse {
    username: String,
    roles: Vec<String>,
    csrf_token: String,
    expires_in_seconds: u64,
}

#[derive(FromRow, Serialize)]
struct CurrentUser {
    id: Uuid,
    username: String,
    roles: Vec<String>,
}

const fn rdp_port() -> u16 {
    3389
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let state = Arc::new(load_state().await?);
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/me", get(current_user))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/rdp/targets", get(list_targets))
        .route("/api/v1/rdp/sessions", post(launch_session))
        .route(
            "/api/v1/admin/rdp/targets",
            get(list_admin_targets).post(create_target),
        )
        .route(
            "/api/v1/admin/rdp/policies",
            get(list_admin_policies).post(create_policy),
        )
        .route(
            "/api/v1/admin/rdp/bindings",
            get(list_admin_bindings).post(create_binding),
        )
        .route(
            "/api/v1/admin/credentials",
            get(list_credentials).post(create_credential_reference),
        )
        .fallback_service(ServeDir::new("web").append_index_html_on_directories(true))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let address: SocketAddr = env::var("PORTAL_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "RDP policy portal listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn load_state() -> Result<AppState, String> {
    let bearer_token = required("PORTAL_BEARER_TOKEN")?;
    if bearer_token.len() < 32 {
        return Err("PORTAL_BEARER_TOKEN must contain at least 32 characters".into());
    }
    let secret = hex::decode(required("GUACAMOLE_JSON_SECRET_KEY")?)
        .map_err(|error| format!("GUACAMOLE_JSON_SECRET_KEY is not hexadecimal: {error}"))?;
    let json_secret: [u8; 16] = secret
        .try_into()
        .map_err(|_| "GUACAMOLE_JSON_SECRET_KEY must be exactly 32 hexadecimal digits")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&required("DATABASE_URL")?)
        .await
        .map_err(|error| format!("database connection failed: {error}"))?;
    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .map_err(|error| format!("database migration failed: {error}"))?;
    let state = AppState {
        pool,
        bearer_token: bearer_token.into(),
        portal_user: required("PORTAL_USER")?.into(),
        allowed_origin: env::var("PORTAL_ALLOWED_ORIGIN")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".into())
            .into(),
        json_secret,
        guacamole_base_url: env::var("GUACAMOLE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8081/guacamole/".into())
            .into(),
    };
    bootstrap_administrator(&state).await?;
    bootstrap_lab_access(&state).await?;
    Ok(state)
}

async fn bootstrap_lab_access(state: &AppState) -> Result<(), String> {
    let hostname = required("RDP_TARGET_HOST")?;
    let fingerprint = required("RDP_CERTIFICATE_SHA256")?;
    let target_id = Uuid::parse_str("7b6256eb-c6dd-4d92-a4b4-d4ca77054e9b").unwrap();
    let policy_id = Uuid::parse_str("b467c62f-8f65-4749-919e-58cce81a006c").unwrap();
    let target = RdpTarget {
        id: target_id.to_string(),
        name: "Windows Lab".into(),
        hostname: hostname.clone(),
        port: 3389,
        certificate_fingerprint: fingerprint.clone(),
        domain: env::var("RDP_DOMAIN").unwrap_or_default(),
    };
    validate_target(&target)?;
    let mut tx = state.pool.begin().await.map_err(db_error)?;
    sqlx::query("INSERT INTO rdp_targets (id,name,hostname,port,domain,certificate_fingerprint,network_zone,enabled) VALUES ($1,'Windows Lab',$2,3389,$3,$4,'rdp-lab',TRUE) ON CONFLICT (id) DO UPDATE SET hostname=EXCLUDED.hostname,domain=EXCLUDED.domain,certificate_fingerprint=EXCLUDED.certificate_fingerprint,updated_at=now()")
        .bind(target_id).bind(hostname).bind(target.domain).bind(fingerprint).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO rdp_policies (id,name,enabled) VALUES ($1,'Default deny',TRUE) ON CONFLICT (id) DO NOTHING")
        .bind(policy_id).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO rdp_policy_bindings (id,policy_id,target_id,subject_type,subject_id,priority) VALUES ($1,$2,$3,'user',$4,0) ON CONFLICT (policy_id,target_id,subject_type,subject_id) DO NOTHING")
        .bind(Uuid::new_v4()).bind(policy_id).bind(target_id).bind(state.portal_user.as_ref()).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

async fn bootstrap_administrator(state: &AppState) -> Result<(), String> {
    let Ok(username) = env::var("PORTAL_BOOTSTRAP_USERNAME") else {
        return Ok(());
    };
    if username.trim().is_empty() {
        return Ok(());
    }
    let password = required("PORTAL_BOOTSTRAP_PASSWORD")?;
    let secret = match env::var("PORTAL_BOOTSTRAP_TOTP_HEX") {
        Ok(value) if !value.trim().is_empty() => hex::decode(value)
            .map_err(|error| format!("PORTAL_BOOTSTRAP_TOTP_HEX is invalid: {error}"))?,
        _ => Vec::new(),
    };
    if !secret.is_empty() && secret.len() < 20 {
        return Err("bootstrap TOTP secret must contain at least 20 bytes".into());
    }
    let username = username.trim().to_lowercase();
    if username.is_empty() || username.len() > 128 {
        return Err("bootstrap username is invalid".into());
    }
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM portal_users WHERE username=$1)",
    )
    .bind(&username)
    .fetch_one(&state.pool)
    .await
    .map_err(db_error)?;
    if !exists {
        let password_hash = hash_password(&password)?;
        sqlx::query("INSERT INTO portal_users (id,username,password_hash,totp_secret,roles) VALUES ($1,$2,$3,$4,ARRAY['administrator','user'])")
            .bind(Uuid::new_v4()).bind(username).bind(password_hash).bind(secret)
            .execute(&state.pool).await.map_err(db_error)?;
    }
    Ok(())
}

fn session_cookie(headers: &HeaderMap) -> Option<Vec<u8>> {
    let value = headers.get("cookie")?.to_str().ok()?;
    let encoded = value
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("vpn_session="))?;
    URL_SAFE_NO_PAD.decode(encoded).ok()
}

async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let username = request.username.trim().to_lowercase();
    let user = sqlx::query_as::<_, LoginUser>("SELECT id,username,password_hash,totp_secret,roles,(enabled AND (locked_until IS NULL OR locked_until <= now())) AS login_allowed FROM portal_users WHERE username=$1")
        .bind(&username).fetch_optional(&state.pool).await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, db_error(error)))?;
    let valid = user.as_ref().is_some_and(|user| {
        user.login_allowed
            && verify_password(&user.password_hash, &request.password)
            && mfa_satisfied(&user.totp_secret, &request.totp)
    });
    if !valid {
        if let Some(user) = user {
            sqlx::query("UPDATE portal_users SET failed_login_count=failed_login_count+1,locked_until=CASE WHEN failed_login_count+1 >= 5 THEN now()+interval '15 minutes' ELSE locked_until END,updated_at=now() WHERE id=$1")
                .bind(user.id).execute(&state.pool).await.ok();
        }
        return Err((
            StatusCode::UNAUTHORIZED,
            "invalid credentials or MFA code".into(),
        ));
    }
    let user = user.expect("validated user exists");
    let token = random_token::<SESSION_BYTES>();
    let csrf = random_token::<CSRF_BYTES>();
    sqlx::query("UPDATE portal_users SET failed_login_count=0,locked_until=NULL,updated_at=now() WHERE id=$1")
        .bind(user.id).execute(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    sqlx::query("INSERT INTO portal_sessions (id,user_id,token_hash,csrf_hash,source_ip,expires_at,idle_expires_at) VALUES ($1,$2,$3,$4,$5::text::inet,now()+interval '8 hours',now()+interval '30 minutes')")
        .bind(Uuid::new_v4()).bind(user.id).bind(token_hash(&token).to_vec()).bind(token_hash(&csrf).to_vec()).bind(peer.ip().to_string())
        .execute(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    let cookie = format!(
        "vpn_session={}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=28800",
        URL_SAFE_NO_PAD.encode(token)
    );
    let mut response = Json(LoginResponse {
        username: user.username,
        roles: user.roles,
        csrf_token: URL_SAFE_NO_PAD.encode(csrf),
        expires_in_seconds: 28_800,
    })
    .into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "cookie creation failed".into(),
            )
        })?,
    );
    Ok(response)
}

async fn current_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<CurrentUser>> {
    let token =
        session_cookie(&headers).ok_or((StatusCode::UNAUTHORIZED, "login required".into()))?;
    let user = sqlx::query_as::<_, CurrentUser>("SELECT u.id,u.username,u.roles FROM portal_sessions s JOIN portal_users u ON u.id=s.user_id AND u.enabled WHERE s.token_hash=$1 AND s.revoked_at IS NULL AND s.expires_at>now() AND s.idle_expires_at>now()")
        .bind(token_hash(&token).to_vec()).fetch_optional(&state.pool).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?
        .ok_or((StatusCode::UNAUTHORIZED, "session expired".into()))?;
    Ok(Json(user))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    if let Some(token) = session_cookie(&headers) {
        sqlx::query("UPDATE portal_sessions SET revoked_at=now() WHERE token_hash=$1 AND revoked_at IS NULL")
            .bind(token_hash(&token).to_vec()).execute(&state.pool).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_static(
            "vpn_session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
        ),
    );
    Ok(response)
}

async fn health(State(state): State<Arc<AppState>>) -> StatusCode {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(1) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("required environment variable {name} is missing"))
}

fn db_error(error: sqlx::Error) -> String {
    format!("database operation failed: {error}")
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    let value = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if value
        .as_bytes()
        .ct_eq(state.bearer_token.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

fn authorize_mutation(headers: &HeaderMap, state: &AppState) -> ApiResult<()> {
    authorize(headers, state).map_err(|status| (status, "unauthorized".into()))?;
    let origin = headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .ok_or((StatusCode::FORBIDDEN, "origin header required".into()))?;
    if origin != state.allowed_origin.as_ref() {
        return Err((StatusCode::FORBIDDEN, "origin is not allowed".into()));
    }
    Ok(())
}

const ACCESS_SQL: &str = "SELECT t.id,t.name,t.hostname,t.port,t.domain,t.certificate_fingerprint,p.clipboard_to_browser,p.clipboard_to_remote,p.upload,p.download,p.printing,p.audio_output,p.microphone,p.recording,p.maximum_duration_seconds FROM rdp_policy_bindings b JOIN rdp_targets t ON t.id=b.target_id AND t.enabled JOIN rdp_policies p ON p.id=b.policy_id AND p.enabled WHERE b.subject_type='user' AND b.subject_id=$1";

async fn list_targets(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<PublicTarget>>> {
    authorize(&headers, &state).map_err(|status| (status, "unauthorized".into()))?;
    let rows = sqlx::query_as::<_, EffectiveAccess>(&format!(
        "{ACCESS_SQL} ORDER BY b.priority DESC,p.priority DESC,b.id ASC"
    ))
    .bind(state.portal_user.as_ref())
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    let mut seen = HashSet::new();
    Ok(Json(
        rows.into_iter()
            .filter(|row| seen.insert(row.id))
            .map(|row| PublicTarget {
                id: row.id,
                name: row.name.clone(),
                policy: row.policy(),
            })
            .collect(),
    ))
}

async fn effective_access(
    state: &AppState,
    target_id: Uuid,
) -> Result<Option<EffectiveAccess>, sqlx::Error> {
    sqlx::query_as::<_, EffectiveAccess>(&format!(
        "{ACCESS_SQL} AND t.id=$2 ORDER BY b.priority DESC,p.priority DESC,b.id ASC LIMIT 1"
    ))
    .bind(state.portal_user.as_ref())
    .bind(target_id)
    .fetch_optional(&state.pool)
    .await
}

async fn launch_session(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<LaunchRequest>,
) -> ApiResult<impl IntoResponse> {
    authorize_mutation(&headers, &state)?;
    let access = effective_access(&state, request.target_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    let Some(access) = access else {
        audit(
            &state,
            peer,
            "rdp.session.launch",
            None,
            None,
            "denied",
            json!({"reason":"no effective policy", "requested_target_id": request.target_id}),
        )
        .await;
        return Err((StatusCode::FORBIDDEN, "access denied by default".into()));
    };
    let target = access.target();
    let policy = access.policy();
    let session_id = Uuid::new_v4();
    let credentials = RdpCredentials {
        username: request.rdp_username,
        password: request.rdp_password,
    };
    let data = encrypted_json_auth_with_credentials(
        &state.json_secret,
        &state.portal_user,
        &target,
        &policy,
        &session_id.to_string(),
        Duration::from_secs(30),
        Some(&credentials),
    )
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let snapshot = serde_json::to_value(&policy)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    sqlx::query("INSERT INTO rdp_sessions (id,user_id,target_id,policy_snapshot,state,source_ip,started_at) VALUES ($1,$2,$3,$4,'active',$5::text::inet,now())")
        .bind(session_id).bind(state.portal_user.as_ref()).bind(access.id).bind(snapshot).bind(peer.ip().to_string()).execute(&state.pool).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    audit(
        &state,
        peer,
        "rdp.session.launch",
        Some(access.id),
        Some(session_id),
        "allowed",
        json!({}),
    )
    .await;
    let guacamole_url = format!(
        "{}?data={}",
        state.guacamole_base_url,
        urlencoding::encode(&data)
    );
    Ok((StatusCode::CREATED, Json(LaunchResponse { session_id, expires_in_seconds: 30, guacamole_url, replay_window_notice: "Guacamole JSON assertions can be replayed until expiration; production requires a one-time authentication extension" })))
}

async fn audit(
    state: &AppState,
    peer: SocketAddr,
    action: &str,
    target_id: Option<Uuid>,
    session_id: Option<Uuid>,
    outcome: &str,
    details: serde_json::Value,
) {
    if let Err(error) = sqlx::query("INSERT INTO rdp_audit_events (id,actor_id,action,target_id,session_id,source_ip,outcome,details) VALUES ($1,$2,$3,$4,$5,$6::text::inet,$7,$8)")
        .bind(Uuid::new_v4()).bind(state.portal_user.as_ref()).bind(action).bind(target_id).bind(session_id).bind(peer.ip().to_string()).bind(outcome).bind(details).execute(&state.pool).await {
        tracing::error!(%error, "failed to persist audit event");
    }
}

async fn list_admin_targets(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<AdminTarget>>> {
    authorize(&headers, &state).map_err(|status| (status, "unauthorized".into()))?;
    let rows = sqlx::query_as::<_, AdminTarget>("SELECT id,name,hostname,port,domain,certificate_fingerprint,network_zone,credential_ref,enabled FROM rdp_targets ORDER BY name,id")
        .fetch_all(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    Ok(Json(rows))
}

async fn list_admin_policies(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<AdminPolicy>>> {
    authorize(&headers, &state).map_err(|status| (status, "unauthorized".into()))?;
    let rows = sqlx::query_as::<_, AdminPolicy>("SELECT id,name,priority,clipboard_to_browser,clipboard_to_remote,upload,download,printing,audio_output,microphone,maximum_duration_seconds,enabled FROM rdp_policies ORDER BY priority DESC,name,id")
        .fetch_all(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    Ok(Json(rows))
}

async fn list_admin_bindings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<AdminBinding>>> {
    authorize(&headers, &state).map_err(|status| (status, "unauthorized".into()))?;
    let rows = sqlx::query_as::<_, AdminBinding>("SELECT id,policy_id,target_id,subject_type,subject_id,priority FROM rdp_policy_bindings ORDER BY priority DESC,id")
        .fetch_all(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    Ok(Json(rows))
}

async fn list_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<CredentialReferenceRow>>> {
    authorize(&headers, &state).map_err(|status| (status, "unauthorized".into()))?;
    let rows = sqlx::query_as::<_, CredentialReferenceRow>("SELECT id,name,provider,external_ref,status,last_rotated_at::text AS last_rotated_at FROM credential_references ORDER BY name,id")
        .fetch_all(&state.pool).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, db_error(e)))?;
    Ok(Json(rows))
}

async fn create_credential_reference(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CredentialReferenceRequest>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    authorize_mutation(&headers, &state)?;
    let provider = match request.provider.as_str() {
        "local_encrypted" => CredentialProvider::LocalEncrypted,
        "hcp_vault_secrets" => CredentialProvider::HcpVaultSecrets,
        "azure_key_vault" => CredentialProvider::AzureKeyVault,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "unsupported credential provider".into(),
            ))
        }
    };
    CredentialReferenceInput {
        name: request.name.clone(),
        provider,
        external_ref: request.external_ref.clone(),
    }
    .validate()
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO credential_references (id,name,provider,external_ref) VALUES ($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(request.name)
    .bind(request.provider)
    .bind(request.external_ref)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::CONFLICT, db_error(e)))?;
    Ok((StatusCode::CREATED, Json(json!({"id": id}))))
}

async fn create_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<TargetInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    authorize_mutation(&headers, &state)?;
    let id = input.id.unwrap_or_else(Uuid::new_v4);
    validate_target(&RdpTarget {
        id: id.to_string(),
        name: input.name.clone(),
        hostname: input.hostname.clone(),
        port: input.port,
        domain: input.domain.clone(),
        certificate_fingerprint: input.certificate_fingerprint.clone(),
    })
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if input.network_zone.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "network_zone is required".into()));
    }
    sqlx::query("INSERT INTO rdp_targets (id,name,hostname,port,domain,certificate_fingerprint,network_zone,enabled) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(id).bind(input.name).bind(input.hostname).bind(input.port as i32).bind(input.domain).bind(input.certificate_fingerprint).bind(input.network_zone).bind(input.enabled)
        .execute(&state.pool).await.map_err(|e| (StatusCode::CONFLICT, db_error(e)))?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

async fn create_policy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<PolicyInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    authorize_mutation(&headers, &state)?;
    if input.name.trim().is_empty()
        || !(1..=28_800).contains(&input.policy.maximum_duration_seconds)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "valid name and duration are required".into(),
        ));
    }
    let id = input.id.unwrap_or_else(Uuid::new_v4);
    let p = input.policy;
    sqlx::query("INSERT INTO rdp_policies (id,name,priority,clipboard_to_browser,clipboard_to_remote,upload,download,printing,audio_output,microphone,recording,maximum_duration_seconds,enabled) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(id).bind(input.name).bind(input.priority).bind(p.clipboard_to_browser).bind(p.clipboard_to_remote).bind(p.upload).bind(p.download).bind(p.printing).bind(p.audio_output).bind(p.microphone).bind(p.recording).bind(p.maximum_duration_seconds as i32).bind(input.enabled)
        .execute(&state.pool).await.map_err(|e| (StatusCode::CONFLICT, db_error(e)))?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

async fn create_binding(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<BindingInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    authorize_mutation(&headers, &state)?;
    if input.subject_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "subject_id is required".into()));
    }
    let id = input.id.unwrap_or_else(Uuid::new_v4);
    sqlx::query("INSERT INTO rdp_policy_bindings (id,policy_id,target_id,subject_type,subject_id,priority) VALUES ($1,$2,$3,'user',$4,$5)")
        .bind(id).bind(input.policy_id).bind(input.target_id).bind(input.subject_id).bind(input.priority)
        .execute(&state.pool).await.map_err(|e| (StatusCode::CONFLICT, db_error(e)))?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

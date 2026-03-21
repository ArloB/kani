use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppError;
 
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id:        i64,
    pub username:  String,
    pub email:     String,
    pub is_active: bool,
    pub roles:     Vec<String>,
 
    /// Never serialised to the client.
    #[serde(skip)]
    pub password_hash: String,
    #[serde(skip)]
    pub change_id: Vec<u8>,
}
 
impl AuthUser for User {
    type Id = i64;
 
    fn id(&self) -> i64 {
        self.id
    }
 
    fn session_auth_hash(&self) -> &[u8] {
        &self.change_id
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}
 
pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[derive(Clone)]
pub struct AuthBackend {
    db: SqlitePool,
}
 
impl AuthBackend {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }
 
    /// Fetch a user row by ID.
    async fn fetch_user_by_id(&self, id: i64) -> Result<Option<User>, AppError> {
        struct Row {
            id:            i64,
            username:      String,
            email:         String,
            password_hash: String,
            change_id:     Vec<u8>,
            is_active:     bool,
        }
 
        let Some(row) = sqlx::query_as!(
            Row,
            r#"SELECT id, username, email, password_hash,
                      change_id as "change_id: Vec<u8>",
                      is_active as "is_active: bool"
               FROM users WHERE id = ?"#,
            id
        )
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
 
        let roles = self.fetch_roles(row.id).await?;
 
        Ok(Some(User {
            id:            row.id,
            username:      row.username,
            email:         row.email,
            password_hash: row.password_hash,
            change_id:     row.change_id,
            is_active:     row.is_active,
            roles,
        }))
    }
 
    /// Fetch a user row by username OR email (whichever matches).
    async fn fetch_user_by_identity(&self, identity: &str) -> Result<Option<User>, AppError> {
        struct Row {
            id:            i64,
            username:      String,
            email:         String,
            password_hash: String,
            change_id:     Vec<u8>,
            is_active:     bool,
        }
 
        let Some(row) = sqlx::query_as!(
            Row,
            r#"SELECT id, username, email, password_hash,
                      change_id as "change_id: Vec<u8>",
                      is_active as "is_active: bool"
               FROM users
               WHERE username = ? OR email = ?
               LIMIT 1"#,
            identity,
            identity
        )
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
 
        let roles = self.fetch_roles(row.id).await?;
 
        Ok(Some(User {
            id:            row.id,
            username:      row.username,
            email:         row.email,
            password_hash: row.password_hash,
            change_id:     row.change_id,
            is_active:     row.is_active,
            roles,
        }))
    }
 
    /// Load the role slugs for `user_id`.
    async fn fetch_roles(&self, user_id: i64) -> Result<Vec<String>, AppError> {
        let roles = sqlx::query_scalar!(
            "SELECT role_slug FROM user_roles WHERE user_id = ? ORDER BY role_slug",
            user_id
        )
        .fetch_all(&self.db)
        .await?;
        Ok(roles)
    }
 
    /// Returns the number of registered users.
    pub async fn user_count(&self) -> Result<i64, AppError> {
        Ok(sqlx::query_scalar!("SELECT COUNT(*) FROM users")
            .fetch_one(&self.db)
            .await?)
    }
 
    /// Creates a new user, hashes their password, and assigns the `user` role.
    /// Returns the created `User`.
    pub async fn create_user(
        &self,
        username: &str,
        email:    &str,
        password: &str,
    ) -> Result<User, AppError> {
        let password_hash = hash_password(password)?;
        let change_id     = fresh_change_id();
 
        let id = sqlx::query_scalar!(
            "INSERT INTO users (username, email, password_hash, change_id)
             VALUES (?, ?, ?, ?)
             RETURNING id",
            username,
            email,
            password_hash,
            change_id,
        )
        .fetch_one(&self.db)
        .await?;
 
        sqlx::query!(
            "INSERT OR IGNORE INTO user_roles (user_id, role_slug) VALUES (?, 'user')",
            id
        )
        .execute(&self.db)
        .await?;
 
        self.fetch_user_by_id(id)
            .await?
            .ok_or_else(|| AppError::SqlxError(sqlx::Error::RowNotFound))
    }
 
    /// Changes a user's password and simultaneously rotates `change_id`,
    /// invalidating all existing sessions.
    pub async fn change_password(
        &self,
        user_id:      i64,
        new_password: &str,
    ) -> Result<(), AppError> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".kani_admin_password");

        if let Err(e) = std::fs::remove_file(&path)
        && e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("Failed to delete temporary admin password file: {}", e);
        }

        let hash      = hash_password(new_password)?;
        let change_id = fresh_change_id();
        sqlx::query!(
            "UPDATE users SET password_hash = ?, change_id = ? WHERE id = ?",
            hash,
            change_id,
            user_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
 
    /// Rotates `change_id` invalidating all active sessions.
    pub async fn cycle_change_id(&self, user_id: i64) -> Result<(), AppError> {
        let change_id = fresh_change_id();
        sqlx::query!(
            "UPDATE users SET change_id = ? WHERE id = ?",
            change_id,
            user_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
 
    /// Activates or deactivates a user account.
    /// Deactivating also rotates `change_id` to terminate live sessions.
    pub async fn set_active(&self, user_id: i64, active: bool) -> Result<(), AppError> {
        if !active {
            self.cycle_change_id(user_id).await?;
        }
        sqlx::query!(
            "UPDATE users SET is_active = ? WHERE id = ?",
            active,
            user_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
 
    /// Grants a role to a user.  `granted_by` is the admin's user id.
    pub async fn grant_role(
        &self,
        user_id:    i64,
        role_slug:  &str,
        granted_by: Option<i64>,
    ) -> Result<(), AppError> {
        sqlx::query!(
            "INSERT OR IGNORE INTO user_roles (user_id, role_slug, granted_by)
             VALUES (?, ?, ?)",
            user_id,
            role_slug,
            granted_by
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
 
    /// Revokes a role from a user.
    pub async fn revoke_role(&self, user_id: i64, role_slug: &str) -> Result<(), AppError> {
        sqlx::query!(
            "DELETE FROM user_roles WHERE user_id = ? AND role_slug = ?",
            user_id,
            role_slug
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
 
    /// Lists all users (without password hashes or change_id).
    pub async fn list_users(&self) -> Result<Vec<User>, AppError> {
        let ids = sqlx::query_scalar!("SELECT id FROM users ORDER BY id")
            .fetch_all(&self.db)
            .await?;
 
        let mut users = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(u) = self.fetch_user_by_id(id).await? {
                users.push(u);
            }
        }
        Ok(users)
    }
}
 
impl AuthnBackend for AuthBackend {
    type User        = User;
    type Credentials = Credentials;
    type Error       = AppError;
 
    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let Some(user) = self.fetch_user_by_identity(&creds.username).await? else {
            return Ok(None);
        };
 
        if !user.is_active {
            return Ok(None);
        }
 
        if !verify_password(&creds.password, &user.password_hash)? {
            return Ok(None);
        }
 
        let _ = sqlx::query!(
            "UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = ?",
            user.id
        )
        .execute(&self.db)
        .await;
 
        Ok(Some(user))
    }
 
    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        self.fetch_user_by_id(*user_id).await
    }
}
 
impl User {
    /// Returns `true` if the user holds `role_slug` **or any ancestor role**.
    pub fn has_role(&self, role_slug: &str) -> bool {
        self.roles.iter().any(|r| r == role_slug)
    }
 
    /// Convenience: returns `true` if the user holds the `admin` role.
    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}
  
/// Axum middleware that enforces authentication on all non-public routes.
pub async fn auth_guard(auth: AuthSession, request: Request, next: Next) -> Response {
    let path = request.uri().path();
 
    if is_public_path(path) {
        return next.run(request).await;
    }
 
    match &auth.user {
        None => {
            if path.starts_with("/rest/") || path.starts_with("/api/") {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Authentication required" })),
                )
                    .into_response();
            }
            Redirect::to("/login").into_response()
        }
        Some(user) if !user.is_active => {
            if path.starts_with("/rest/") || path.starts_with("/api/") {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Account is inactive" })),
                )
                    .into_response();
            }
            Redirect::to("/login").into_response()
        }
        Some(_) => next.run(request).await,
    }
}
 
/// Returns `true` for paths that are always accessible without a session.
fn is_public_path(path: &str) -> bool {
    path == "/login"
        || path.starts_with("/rest/auth/")
        || path.starts_with("/pkg/")
        || path == "/favicon.ico"
}
 
/// Hashes a plaintext password using Argon2id.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}
 
/// Verifies a plaintext password against a stored Argon2 hash.
pub fn verify_password(
    password: &str,
    hash:     &str,
) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
 
/// Generates 16 fresh random bytes for use as `change_id`.
pub fn fresh_change_id() -> Vec<u8> {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.to_vec()
}
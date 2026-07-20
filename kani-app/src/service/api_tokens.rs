use crate::error::{Result, ServiceError};
use crate::ids::UserId;
use crate::permissions::Permission;
use crate::service::AppService;

const API_TOKEN_PREFIX: &str = "kani_";
const API_TOKEN_BYTES: usize = 32;

pub struct ApiToken {
    pub id: String,
    pub user_id: UserId,
    pub name: String,
    pub scopes: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

pub struct CreatedApiToken {
    pub token: ApiToken,
    pub raw_token: String,
}

pub struct ApiTokenAuth {
    pub user_id: UserId,
    pub scopes: Vec<Permission>,
}

pub(crate) fn generate_raw_token() -> String {
    let bytes: [u8; API_TOKEN_BYTES] = rand::random();
    format!("{API_TOKEN_PREFIX}{}", hex::encode(bytes))
}

pub(crate) fn hash_token(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(raw.as_bytes()))
}

pub(crate) fn parse_scopes(scopes: &str) -> Vec<Permission> {
    scopes
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect()
}

impl AppService {
    pub async fn create_api_token(
        &self,
        user_id: UserId,
        name: &str,
        expires_in_days: Option<u32>,
    ) -> Result<CreatedApiToken> {
        let raw_token = generate_raw_token();
        let hash = hash_token(&raw_token);
        let uid = user_id.0;
        let days = expires_in_days.map(i64::from);

        sqlx::query!(
            r#"
            INSERT INTO api_tokens (user_id, name, token_hash, expires_at)
            VALUES (?, ?, ?, CASE WHEN ? IS NULL THEN NULL ELSE unixepoch() + ? * 86400 END)
            "#,
            uid,
            name,
            hash,
            days,
            days,
        )
        .execute(&self.db)
        .await?;

        let row = sqlx::query!(
            r#"
            SELECT id AS "id!", user_id, name, scopes, created_at,
                   last_used_at, expires_at, revoked_at
            FROM api_tokens WHERE token_hash = ?
            "#,
            hash,
        )
        .fetch_one(&self.db)
        .await?;

        Ok(CreatedApiToken {
            token: ApiToken {
                id: row.id,
                user_id: UserId(row.user_id),
                name: row.name,
                scopes: row.scopes,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
            },
            raw_token,
        })
    }

    pub async fn list_api_tokens(&self, user_id: UserId) -> Result<Vec<ApiToken>> {
        let uid = user_id.0;
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!", user_id, name, scopes, created_at,
                   last_used_at, expires_at, revoked_at
            FROM api_tokens
            WHERE user_id = ? AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
            uid,
        )
        .fetch_all(&self.db_read)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ApiToken {
                id: row.id,
                user_id: UserId(row.user_id),
                name: row.name,
                scopes: row.scopes,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
            })
            .collect())
    }

    pub async fn revoke_api_token(&self, user_id: UserId, token_id: &str) -> Result<()> {
        let uid = user_id.0;
        let affected = sqlx::query!(
            r#"
            UPDATE api_tokens SET revoked_at = unixepoch()
            WHERE id = ? AND user_id = ? AND revoked_at IS NULL
            "#,
            token_id,
            uid,
        )
        .execute(&self.db)
        .await?
        .rows_affected();

        if affected == 0 {
            return Err(ServiceError::NotFound(format!("api token {token_id}")));
        }
        Ok(())
    }

    pub async fn authenticate_api_token(&self, raw_token: &str) -> Result<Option<ApiTokenAuth>> {
        if !raw_token.starts_with(API_TOKEN_PREFIX) {
            return Ok(None);
        }
        let hash = hash_token(raw_token);

        let row = sqlx::query!(
            r#"
            SELECT id AS "id!", user_id, scopes
            FROM api_tokens
            WHERE token_hash = ? AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > unixepoch())
            "#,
            hash,
        )
        .fetch_optional(&self.db_read)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let id = row.id;
        sqlx::query!(
            r#"
            UPDATE api_tokens SET last_used_at = unixepoch()
            WHERE id = ? AND (last_used_at IS NULL OR last_used_at < unixepoch() - 60)
            "#,
            id,
        )
        .execute(&self.db)
        .await?;

        Ok(Some(ApiTokenAuth {
            user_id: UserId(row.user_id),
            scopes: parse_scopes(&row.scopes),
        }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::permissions::Opds;

    #[test]
    fn generate_raw_token_has_prefix_and_length() {
        let token = generate_raw_token();
        assert!(token.starts_with("kani_"));
        assert_eq!(token.len(), 5 + API_TOKEN_BYTES * 2);
    }

    #[test]
    fn generate_raw_token_is_distinct() {
        assert_ne!(generate_raw_token(), generate_raw_token());
    }

    #[test]
    fn hash_token_is_deterministic() {
        assert_eq!(hash_token("kani_abc"), hash_token("kani_abc"));
    }

    #[test]
    fn hash_token_is_distinct_per_input() {
        assert_ne!(hash_token("kani_abc"), hash_token("kani_abd"));
    }

    #[test]
    fn parse_scopes_round_trip() {
        let scopes = parse_scopes("opds:read opds:progress");
        assert_eq!(
            scopes,
            vec![
                Permission::Opds(Opds::Read),
                Permission::Opds(Opds::Progress),
            ]
        );
    }

    #[test]
    fn parse_scopes_ignores_garbage() {
        let scopes = parse_scopes("opds:read not-a-perm opds:progress");
        assert_eq!(
            scopes,
            vec![
                Permission::Opds(Opds::Read),
                Permission::Opds(Opds::Progress),
            ]
        );
    }
}

//! User management system for multi-user support.
//!
//! Provides user accounts, channel identity mapping, and webhook tokens.
//! Used for permission enforcement and webhook ingress authentication.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{Database, UserIdentityRow, UserRow, WebhookTokenRow};

pub use crate::storage::{
    UserIdentityRow as UserIdentity, UserRow as User, WebhookTokenRow as WebhookToken,
};

/// User roles for permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
    Guest,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
            Role::Guest => "guest",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "user" => Some(Role::User),
            "guest" => Some(Role::Guest),
            _ => None,
        }
    }

    /// Check if this role has at least the given permission level.
    pub fn has_permission(&self, required: Role) -> bool {
        use Role::*;
        matches!(
            (self, required),
            (Admin, Admin)
                | (Admin, User)
                | (Admin, Guest)
                | (User, User)
                | (User, Guest)
                | (Guest, Guest)
        )
    }
}

/// Parsed user info with resolved roles.
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub roles: Vec<Role>,
}

impl UserInfo {
    /// Parse from a database row.
    pub fn from_row(row: &UserRow) -> Result<Self> {
        let roles: Vec<String> =
            serde_json::from_str(&row.roles).unwrap_or_else(|_| vec!["user".to_string()]);

        let roles: Vec<Role> = roles.iter().filter_map(|r| Role::from_str(r)).collect();

        // Default to "user" role if none parsed
        let roles = if roles.is_empty() {
            vec![Role::User]
        } else {
            roles
        };

        Ok(Self {
            id: row.id.clone(),
            username: row.username.clone(),
            roles,
        })
    }

    /// Check if user has a specific role.
    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    /// Check if user has at least the required permission level.
    pub fn has_permission(&self, required: Role) -> bool {
        self.roles.iter().any(|r| r.has_permission(required))
    }
}

/// User manager for high-level operations.
pub struct UserManager {
    db: Database,
}

impl UserManager {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Create a new user with default "user" role.
    pub async fn create_user(&self, username: &str) -> Result<UserInfo> {
        let id = Uuid::new_v4().to_string();
        self.db.create_user(&id, username, &["user"]).await?;

        Ok(UserInfo {
            id,
            username: username.to_string(),
            roles: vec![Role::User],
        })
    }

    /// Create a new admin user.
    pub async fn create_admin(&self, username: &str) -> Result<UserInfo> {
        let id = Uuid::new_v4().to_string();
        self.db.create_user(&id, username, &["admin"]).await?;

        Ok(UserInfo {
            id,
            username: username.to_string(),
            roles: vec![Role::Admin],
        })
    }

    /// Get a user by ID.
    pub async fn get_user(&self, id: &str) -> Result<Option<UserInfo>> {
        let row = self.db.load_user(id).await?;
        match row {
            Some(r) => Ok(Some(UserInfo::from_row(&r)?)),
            None => Ok(None),
        }
    }

    /// Get a user by username.
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<UserInfo>> {
        let row = self.db.load_user_by_username(username).await?;
        match row {
            Some(r) => Ok(Some(UserInfo::from_row(&r)?)),
            None => Ok(None),
        }
    }

    /// Look up a user by their channel identity (e.g., Telegram user ID).
    pub async fn lookup_by_channel(
        &self,
        channel: &str,
        platform_id: &str,
    ) -> Result<Option<UserInfo>> {
        let row = self
            .db
            .lookup_user_by_identity(channel, platform_id)
            .await?;
        match row {
            Some(r) => Ok(Some(UserInfo::from_row(&r)?)),
            None => Ok(None),
        }
    }

    /// Look up a user by webhook token.
    pub async fn lookup_by_webhook_token(&self, token: &str) -> Result<Option<UserInfo>> {
        let row = self.db.lookup_user_by_webhook_token(token).await?;
        match row {
            Some(r) => {
                // Update last_used timestamp
                self.db.touch_webhook_token(token).await?;
                Ok(Some(UserInfo::from_row(&r)?))
            }
            None => Ok(None),
        }
    }

    /// Link a channel identity to a user.
    pub async fn link_identity(
        &self,
        user_id: &str,
        channel: &str,
        platform_id: &str,
        display_name: Option<&str>,
    ) -> Result<()> {
        self.db
            .add_user_identity(user_id, channel, platform_id, display_name)
            .await
    }

    /// Unlink a channel identity from a user.
    pub async fn unlink_identity(
        &self,
        user_id: &str,
        channel: &str,
        platform_id: &str,
    ) -> Result<bool> {
        self.db
            .remove_user_identity(user_id, channel, platform_id)
            .await
    }

    /// Create a webhook token for a user with a given scope (e.g., "admin", "read").
    pub async fn create_webhook_token(
        &self,
        user_id: &str,
        name: &str,
        scope: &str,
    ) -> Result<String> {
        // Generate a secure random token
        let token = format!("wh_{}", Uuid::new_v4().simple());
        self.db
            .create_webhook_token(&token, user_id, name, scope)
            .await?;
        Ok(token)
    }

    /// List all users.
    pub async fn list_users(&self) -> Result<Vec<UserInfo>> {
        let rows = self.db.load_all_users().await?;
        rows.iter().map(UserInfo::from_row).collect()
    }

    /// Update user roles.
    pub async fn update_roles(&self, user_id: &str, roles: &[Role]) -> Result<bool> {
        let role_strs: Vec<&str> = roles.iter().map(|r| r.as_str()).collect();
        self.db.update_user_roles(user_id, &role_strs).await
    }

    /// Delete a user and all their identities/tokens.
    pub async fn delete_user(&self, user_id: &str) -> Result<bool> {
        self.db.delete_user(user_id).await
    }

    /// Get database reference for direct access.
    pub fn db(&self) -> &Database {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.has_permission(Role::Admin));
        assert!(Role::Admin.has_permission(Role::User));
        assert!(Role::Admin.has_permission(Role::Guest));

        assert!(!Role::User.has_permission(Role::Admin));
        assert!(Role::User.has_permission(Role::User));
        assert!(Role::User.has_permission(Role::Guest));

        assert!(!Role::Guest.has_permission(Role::Admin));
        assert!(!Role::Guest.has_permission(Role::User));
        assert!(Role::Guest.has_permission(Role::Guest));
    }

    #[test]
    fn test_user_info_from_row() {
        let row = UserRow {
            id: "test-id".to_string(),
            username: "testuser".to_string(),
            roles: r#"["admin","user"]"#.to_string(),
            password_hash: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            metadata: "{}".to_string(),
        };

        let info = UserInfo::from_row(&row).unwrap();
        assert_eq!(info.id, "test-id");
        assert_eq!(info.username, "testuser");
        assert!(info.has_role(Role::Admin));
        assert!(info.has_role(Role::User));
    }
}

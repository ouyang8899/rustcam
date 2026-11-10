use rbatis::rbatis::RBatis;
use rbs::Value;
use rbdc_pg::PgDriver;
use async_trait::async_trait;

use argon2::PasswordHasher;
use uuid::Uuid;
use crate::error::Result;
use super::models::{Permission, Role, User};
use super::repository::{UserRepository, RoleRepository, PermissionRepository};

pub struct PostgresAuthRepository {
    rb: RBatis,
}

impl PostgresAuthRepository {

    pub fn with_rb(rb: RBatis) -> Self {
        Self { rb }
    }

    pub async fn init(&self, default_username: &str, default_password: &str) {
        self.init_tables().await;
        self.seed_default_data(default_username, default_password).await;
    }

    pub async fn new(database_url: &str, default_username: &str, default_password: &str) -> anyhow::Result<Self> {
        let rb = RBatis::new();
        let _init_res = rb.init(PgDriver {}, database_url);
        let repo = Self { rb };
        repo.init_tables().await;
        repo.seed_default_data(default_username, default_password).await;
        Ok(repo)
    }

    async fn init_tables(&self) {
        let tables = [
            "CREATE TABLE IF NOT EXISTS permissions (id UUID PRIMARY KEY, name VARCHAR(255) NOT NULL UNIQUE, description TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
            "CREATE TABLE IF NOT EXISTS roles (id UUID PRIMARY KEY, name VARCHAR(255) NOT NULL UNIQUE, description TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
            "CREATE TABLE IF NOT EXISTS users (id UUID PRIMARY KEY, username VARCHAR(255) NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
            "CREATE TABLE IF NOT EXISTS user_roles (user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE, PRIMARY KEY (user_id, role_id))",
            "CREATE TABLE IF NOT EXISTS role_permissions (role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE, permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE, PRIMARY KEY (role_id, permission_id))",
            "CREATE TABLE IF NOT EXISTS role_inheritance (parent_role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE, child_role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE, PRIMARY KEY (parent_role_id, child_role_id), CHECK (parent_role_id <> child_role_id))",
        ];
        for sql in tables {
            if let Err(e) = self.rb.exec(sql, vec![]).await {
                tracing::warn!("[AuthDB] init_tables warning (may already exist): {}", e);
            }
        }
        let indexes = [
            "CREATE INDEX IF NOT EXISTS idx_role_permissions_permission ON role_permissions (permission_id)",
            "CREATE INDEX IF NOT EXISTS idx_user_roles_role ON user_roles (role_id)",
            "CREATE INDEX IF NOT EXISTS idx_role_inheritance_child ON role_inheritance (child_role_id)",
        ];
        for sql in indexes {
            if let Err(e) = self.rb.exec(sql, vec![]).await {
                tracing::warn!("[AuthDB] init_indexes warning: {}", e);
            }
        }
    }

    async fn seed_default_data(&self, default_username: &str, default_password: &str) {
        let count: Vec<Value> = match self.rb.exec_decode("SELECT COUNT(*) as c FROM users", vec![]).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[AuthDB] seed_default_data skipped (table not ready): {}", e);
                return;
            }
        };
        if !count.is_empty() && count[0]["c"].as_i64().unwrap_or(0) > 0 { return; }

        let now = chrono::Utc::now().to_rfc3339();
        let admin_role_id = Uuid::new_v4();
        let admin_user_id = Uuid::new_v4();

        if let Err(e) = self.rb.exec("INSERT INTO roles (id, name, description, created_at, updated_at) VALUES ($1::uuid, $2, $3, $4, $5)",
                     vec![rbs::value![admin_role_id.to_string()], rbs::value!["Admin"], rbs::value!["Administrator"], rbs::value![now.clone()], rbs::value![now.clone()]]).await {
            tracing::warn!("[AuthDB] seed_default_data (role may exist): {}", e);
        }

        let _salt = password_hash::phc::SaltString::generate();
        let hash = match argon2::Argon2::default().hash_password(default_password.as_bytes()) {
            Ok(h) => h.to_string(),
            Err(e) => {
                tracing::warn!("[AuthDB] seed_default_data (hash failed): {}", e);
                return;
            }
        };

        if let Err(e) = self.rb.exec("INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES ($1::uuid, $2, $3, $4, $5)",
                     vec![rbs::value![admin_user_id.to_string()], rbs::value![default_username], rbs::value![hash], rbs::value![now.clone()], rbs::value![now.clone()]]).await {
            tracing::warn!("[AuthDB] seed_default_data (user may exist): {}", e);
        }

        if let Err(e) = self.rb.exec("INSERT INTO user_roles (user_id, role_id) VALUES ($1::uuid, $2::uuid)",
                     vec![rbs::value![admin_user_id.to_string()], rbs::value![admin_role_id.to_string()]]).await {
            tracing::warn!("[AuthDB] seed_default_data (user_roles may exist): {}", e);
        }

    }
}

impl Clone for PostgresAuthRepository {
    fn clone(&self) -> Self { Self { rb: self.rb.clone() } }
}

#[async_trait]
impl UserRepository for PostgresAuthRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT id, username, password_hash, created_at, updated_at FROM users WHERE id = $1::uuid",
            vec![rbs::value![id.to_string()]],
        ).await?;
        Ok(rows.into_iter().next().map(|r| User {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            username: r["username"].as_str().unwrap_or("").to_string(),
            password_hash: r["password_hash"].as_str().unwrap_or("").to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT id, username, password_hash, created_at, updated_at FROM users WHERE username = $1",
            vec![rbs::value![username]],
        ).await?;
        Ok(rows.into_iter().next().map(|r| User {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            username: r["username"].as_str().unwrap_or("").to_string(),
            password_hash: r["password_hash"].as_str().unwrap_or("").to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }))
    }

    async fn create(&self, user: &User) -> Result<()> {
        self.rb.exec("INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES ($1::uuid, $2, $3, $4::timestamptz, $5::timestamptz)",
                     vec![rbs::value![user.id.to_string()], rbs::value![user.username.clone()], rbs::value![user.password_hash.clone()], rbs::value![user.created_at.to_rfc3339()], rbs::value![user.updated_at.to_rfc3339()]]).await?;
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        self.rb.exec("UPDATE users SET username = $1, password_hash = $2, updated_at = $3::timestamptz WHERE id = $4::uuid",
                     vec![rbs::value![user.username.clone()], rbs::value![user.password_hash.clone()], rbs::value![user.updated_at.to_rfc3339()], rbs::value![user.id.to_string()]]).await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.rb.exec("DELETE FROM users WHERE id = $1::uuid", vec![rbs::value![id.to_string()]]).await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<User>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, username, password_hash, created_at, updated_at FROM users", vec![]).await?;
        Ok(rows.into_iter().map(|r| User {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            username: r["username"].as_str().unwrap_or("").to_string(),
            password_hash: r["password_hash"].as_str().unwrap_or("").to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }).collect())
    }

    async fn list_paginated(&self, offset: usize, limit: usize) -> Result<Vec<User>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT id, username, password_hash, created_at, updated_at FROM users \
             ORDER BY created_at, id LIMIT $1 OFFSET $2",
            vec![rbs::value![limit as i64], rbs::value![offset as i64]],
        ).await?;
        Ok(rows.into_iter().map(|r| User {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            username: r["username"].as_str().unwrap_or("").to_string(),
            password_hash: r["password_hash"].as_str().unwrap_or("").to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }).collect())
    }

    async fn count(&self) -> Result<usize> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT count(*) AS total FROM users", vec![]).await?;
        Ok(rows
            .first()
            .and_then(|r| r["total"].as_i64())
            .unwrap_or(0)
            .max(0) as usize)
    }

    async fn get_user_roles(&self, user_id: Uuid) -> Result<Vec<String>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT r.name FROM roles r INNER JOIN user_roles ur ON r.id::uuid = ur.role_id::uuid WHERE ur.user_id = $1::uuid",
            vec![rbs::value![user_id.to_string()]],
        ).await?;
        Ok(rows.into_iter().filter_map(|r| r["name"].as_str().map(String::from)).collect())
    }

    async fn set_user_roles(&self, user_id: Uuid, role_ids: &[Uuid]) -> Result<()> {
        self.rb.exec("DELETE FROM user_roles WHERE user_id = $1::uuid", vec![rbs::value![user_id.to_string()]]).await?;
        for role_id in role_ids {
            self.rb.exec("INSERT INTO user_roles (user_id, role_id) VALUES ($1::uuid, $2::uuid)",
                         vec![rbs::value![user_id.to_string()], rbs::value![role_id.to_string()]]).await?;
        }
        Ok(())
    }

    async fn get_user_role_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT role_id FROM user_roles WHERE user_id = $1::uuid",
            vec![rbs::value![user_id.to_string()]],
        ).await?;
        Ok(rows.into_iter().filter_map(|r| {
            Uuid::parse_str(r["role_id"].as_str().unwrap_or("")).ok()
        }).collect())
    }
}

#[async_trait]
impl RoleRepository for PostgresAuthRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Role>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, name, description, created_at, updated_at FROM roles WHERE id = $1::uuid",
                                                   vec![rbs::value![id.to_string()]]).await?;
        Ok(rows.into_iter().next().map(|r| Role {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, name, description, created_at, updated_at FROM roles WHERE name = $1",
                                                   vec![rbs::value![name]]).await?;
        Ok(rows.into_iter().next().map(|r| Role {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }))
    }

    async fn create(&self, role: &Role) -> Result<()> {
        self.rb.exec("INSERT INTO roles (id, name, description, created_at, updated_at) VALUES ($1::uuid, $2, $3, $4::timestamptz, $5::timestamptz)",
                     vec![rbs::value![role.id.to_string()], rbs::value![role.name.clone()], rbs::value![role.description.clone().unwrap_or_default()], rbs::value![role.created_at.to_rfc3339()], rbs::value![role.updated_at.to_rfc3339()]]).await?;
        Ok(())
    }

    async fn update(&self, role: &Role) -> Result<()> {
        self.rb.exec("UPDATE roles SET name = $1, description = $2, updated_at = $3::timestamptz WHERE id = $4::uuid",
                     vec![rbs::value![role.name.clone()], rbs::value![role.description.clone().unwrap_or_default()], rbs::value![role.updated_at.to_rfc3339()], rbs::value![role.id.to_string()]]).await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.rb.exec("DELETE FROM roles WHERE id = $1::uuid", vec![rbs::value![id.to_string()]]).await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Role>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, name, description, created_at, updated_at FROM roles", vec![]).await?;
        Ok(rows.into_iter().map(|r| Role {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).collect())
    }

    async fn get_role_permissions(&self, role_id: Uuid) -> Result<Vec<String>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT p.name FROM permissions p INNER JOIN role_permissions rp ON p.id::uuid = rp.permission_id::uuid WHERE rp.role_id = $1::uuid",
            vec![rbs::value![role_id.to_string()]]).await?;
        Ok(rows.into_iter().filter_map(|r| r["name"].as_str().map(String::from)).collect())
    }

    async fn set_role_permissions(&self, role_id: Uuid, permission_ids: &[Uuid]) -> Result<()> {
        self.rb.exec("DELETE FROM role_permissions WHERE role_id = $1::uuid", vec![rbs::value![role_id.to_string()]]).await?;
        for perm_id in permission_ids {
            self.rb.exec("INSERT INTO role_permissions (role_id, permission_id) VALUES ($1::uuid, $2::uuid)",
                         vec![rbs::value![role_id.to_string()], rbs::value![perm_id.to_string()]]).await?;
        }
        Ok(())
    }

    async fn list_role_permissions(&self) -> Result<Vec<(String, String)>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT r.name AS role_name, p.name AS permission_name \
             FROM roles r \
             INNER JOIN role_permissions rp ON rp.role_id = r.id \
             INNER JOIN permissions p ON p.id = rp.permission_id",
            vec![]).await?;
        Ok(rows.into_iter().filter_map(|r| {
            let role = r["role_name"].as_str()?.to_string();
            let permission = r["permission_name"].as_str()?.to_string();
            Some((role, permission))
        }).collect())
    }

    async fn list_role_inheritance(&self) -> Result<Vec<(String, String)>> {
        let rows: Vec<Value> = match self.rb.exec_decode(
            "SELECT child.name AS child_name, parent.name AS parent_name \
             FROM role_inheritance ri \
             INNER JOIN roles child ON child.id = ri.child_role_id \
             INNER JOIN roles parent ON parent.id = ri.parent_role_id",
            vec![]).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "[AuthDB] role_inheritance unavailable, continuing without role inheritance; run sql/migrate_role_inheritance.sql"
                );
                return Ok(Vec::new());
            }
        };
        Ok(rows.into_iter().filter_map(|r| {
            let child = r["child_name"].as_str()?.to_string();
            let parent = r["parent_name"].as_str()?.to_string();
            Some((child, parent))
        }).collect())
    }

    async fn get_role_parents(&self, role_id: Uuid) -> Result<Vec<Role>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT parent.id, parent.name, parent.description, parent.created_at, parent.updated_at \
             FROM role_inheritance ri \
             INNER JOIN roles parent ON parent.id = ri.parent_role_id \
             WHERE ri.child_role_id = $1::uuid",
            vec![rbs::value![role_id.to_string()]]).await?;
        Ok(rows.into_iter().map(|r| Role {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).collect())
    }

    async fn set_role_parents(&self, role_id: Uuid, parent_ids: &[Uuid]) -> Result<()> {
        self.rb.exec("DELETE FROM role_inheritance WHERE child_role_id = $1::uuid", vec![rbs::value![role_id.to_string()]]).await?;
        for parent_id in parent_ids {
            self.rb.exec("INSERT INTO role_inheritance (parent_role_id, child_role_id) VALUES ($1::uuid, $2::uuid)",
                         vec![rbs::value![parent_id.to_string()], rbs::value![role_id.to_string()]]).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl PermissionRepository for PostgresAuthRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Permission>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, name, description, created_at FROM permissions WHERE id = $1::uuid",
                                                   vec![rbs::value![id.to_string()]]).await?;
        Ok(rows.into_iter().next().map(|r| Permission {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(),
        }))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Permission>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, name, description, created_at FROM permissions WHERE name = $1",
                                                   vec![rbs::value![name]]).await?;
        Ok(rows.into_iter().next().map(|r| Permission {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(),
        }))
    }

    async fn create(&self, permission: &Permission) -> Result<()> {
        self.rb.exec("INSERT INTO permissions (id, name, description, created_at) VALUES ($1::uuid, $2, $3, $4::timestamptz)",
                     vec![rbs::value![permission.id.to_string()], rbs::value![permission.name.clone()], rbs::value![permission.description.clone().unwrap_or_default()], rbs::value![permission.created_at.to_rfc3339()]]).await?;
        Ok(())
    }

    async fn update(&self, permission: &Permission) -> Result<()> {
        self.rb.exec("UPDATE permissions SET name = $1, description = $2 WHERE id = $3::uuid",
                     vec![rbs::value![permission.name.clone()], rbs::value![permission.description.clone().unwrap_or_default()], rbs::value![permission.id.to_string()]]).await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.rb.exec("DELETE FROM permissions WHERE id = $1::uuid", vec![rbs::value![id.to_string()]]).await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Permission>> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT id, name, description, created_at FROM permissions", vec![]).await?;
        Ok(rows.into_iter().map(|r| Permission {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(),
        }).collect())
    }

    async fn list_paginated(&self, offset: usize, limit: usize) -> Result<Vec<Permission>> {
        let rows: Vec<Value> = self.rb.exec_decode(
            "SELECT id, name, description, created_at FROM permissions \
             ORDER BY name, id LIMIT $1 OFFSET $2",
            vec![rbs::value![limit as i64], rbs::value![offset as i64]],
        ).await?;
        Ok(rows.into_iter().map(|r| Permission {
            id: Uuid::parse_str(r["id"].as_str().unwrap_or("")).unwrap_or(Uuid::nil()),
            name: r["name"].as_str().unwrap_or("").to_string(),
            description: r["description"].as_str().map(String::from),
            created_at: chrono::Utc::now(),
        }).collect())
    }

    async fn count(&self) -> Result<usize> {
        let rows: Vec<Value> = self.rb.exec_decode("SELECT count(*) AS total FROM permissions", vec![]).await?;
        Ok(rows
            .first()
            .and_then(|r| r["total"].as_i64())
            .unwrap_or(0)
            .max(0) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归测试：多参数 SQL 必须逐个绑定参数。
    ///
    /// `rbs::value![a, b]` 会生成一个 JSON 数组（单个 json 参数），
    /// 会导致 `$1::uuid` 这类语句报 "cannot cast type json to uuid"。
    /// 这里用真实数据库跑一遍角色/权限的增删改查，防止再次写错。
    #[tokio::test]
    async fn multi_parameter_writes_bind_each_value() {
        let url = match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => return,
        };

        let repo = PostgresAuthRepository::with_rb({
            let rb = RBatis::new();
            let _ = rb.init(PgDriver {}, &url);
            rb
        });

        let suffix = Uuid::new_v4().to_string();

        let role = Role::new(format!("test_role_{}", suffix), Some("desc".to_string()));
        RoleRepository::create(&repo, &role)
            .await
            .expect("insert role with multiple params");

        let loaded = RoleRepository::find_by_name(&repo, &role.name)
            .await
            .expect("query role")
            .expect("role exists");
        assert_eq!(loaded.id, role.id);

        let mut updated = loaded.clone();
        updated.description = Some("updated".to_string());
        RoleRepository::update(&repo, &updated)
            .await
            .expect("update role with multiple params");

        let permission = Permission::new(format!("test:perm_{}", suffix), None);
        PermissionRepository::create(&repo, &permission)
            .await
            .expect("insert permission");
        RoleRepository::set_role_permissions(&repo, role.id, &[permission.id])
            .await
            .expect("bind role permissions");

        let names = RoleRepository::get_role_permissions(&repo, role.id)
            .await
            .expect("query role permissions");
        assert_eq!(names, vec![permission.name.clone()]);

        let pairs = RoleRepository::list_role_permissions(&repo)
            .await
            .expect("list role permissions");
        assert!(pairs
            .iter()
            .any(|(r, p)| r == &role.name && p == &permission.name));

        RoleRepository::delete(&repo, role.id)
            .await
            .expect("delete role");
        assert!(RoleRepository::find_by_id(&repo, role.id)
            .await
            .expect("query deleted role")
            .is_none());
    }
}
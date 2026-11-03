use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use crate::error::Result;

use super::models::{Permission, Role, User};

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>>;
    async fn find_by_username(&self, username: &str) -> Result<Option<User>>;
    async fn create(&self, user: &User) -> Result<()>;
    async fn update(&self, user: &User) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
    async fn list(&self) -> Result<Vec<User>>;
    async fn get_user_roles(&self, user_id: Uuid) -> Result<Vec<String>>;
    async fn get_user_role_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>>;
    async fn set_user_roles(&self, user_id: Uuid, role_ids: &[Uuid]) -> Result<()>;
}

#[async_trait]
pub trait RoleRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Role>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Role>>;
    async fn create(&self, role: &Role) -> Result<()>;
    async fn update(&self, role: &Role) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
    async fn list(&self) -> Result<Vec<Role>>;
    async fn get_role_permissions(&self, role_id: Uuid) -> Result<Vec<String>>;
    async fn set_role_permissions(&self, role_id: Uuid, permission_ids: &[Uuid]) -> Result<()>;

    /// 一次查出全部「角色 -> 权限」授权关系，避免逐角色查询。
    async fn list_role_permissions(&self) -> Result<Vec<(String, String)>>;

    /// 一次查出全部「子角色 -> 父角色」继承关系。
    async fn list_role_inheritance(&self) -> Result<Vec<(String, String)>>;

    /// 某角色直接继承的父角色。
    async fn get_role_parents(&self, role_id: Uuid) -> Result<Vec<Role>>;

    /// 覆盖式设置某角色直接继承的父角色。
    async fn set_role_parents(&self, role_id: Uuid, parent_ids: &[Uuid]) -> Result<()>;
}

#[async_trait]
pub trait PermissionRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Permission>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Permission>>;
    async fn create(&self, permission: &Permission) -> Result<()>;
    async fn update(&self, permission: &Permission) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
    async fn list(&self) -> Result<Vec<Permission>>;
}

pub struct InMemoryAuthRepository {
    users: DashMap<Uuid, User>,
    username_index: DashMap<String, Uuid>,
    roles: DashMap<Uuid, Role>,
    permissions: DashMap<Uuid, Permission>,
    user_roles: DashMap<Uuid, Vec<Uuid>>,
    role_permissions: DashMap<Uuid, Vec<Uuid>>,
    role_parents: DashMap<Uuid, Vec<Uuid>>,
}

impl Default for InMemoryAuthRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryAuthRepository {
    pub fn new() -> Self {
        Self {
            users: DashMap::new(),
            username_index: DashMap::new(),
            roles: DashMap::new(),
            permissions: DashMap::new(),
            user_roles: DashMap::new(),
            role_permissions: DashMap::new(),
            role_parents: DashMap::new(),
        }
    }
}

#[async_trait]
impl UserRepository for InMemoryAuthRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>> {
        Ok(self.users.get(&id).map(|u| u.clone()))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>> {
        Ok(self.username_index
            .get(username)
            .and_then(|id| self.users.get(&id).map(|u| u.clone())))
    }

    async fn create(&self, user: &User) -> Result<()> {
        self.users.insert(user.id, user.clone());
        self.username_index.insert(user.username.clone(), user.id);
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        self.users.insert(user.id, user.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        if let Some((_, user)) = self.users.remove(&id) {
            self.username_index.remove(&user.username);
            self.user_roles.remove(&id);
        }
        Ok(())
    }

    async fn list(&self) -> Result<Vec<User>> {
        Ok(self.users.iter().map(|u| u.clone()).collect())
    }

    async fn get_user_roles(&self, user_id: Uuid) -> Result<Vec<String>> {
        let role_ids = self.user_roles
            .get(&user_id)
            .map(|r| r.clone())
            .unwrap_or_default();

        let roles: Vec<String> = role_ids
            .iter()
            .filter_map(|id| self.roles.get(id).map(|r| r.name.clone()))
            .collect();

        Ok(roles)
    }

    async fn set_user_roles(&self, user_id: Uuid, role_ids: &[Uuid]) -> Result<()> {
        self.user_roles.insert(user_id, role_ids.to_vec());
        Ok(())
    }

    async fn get_user_role_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>> {
        Ok(self
            .user_roles
            .get(&user_id)
            .map(|r| r.clone())
            .unwrap_or_default())
    }
}

#[async_trait]
impl RoleRepository for InMemoryAuthRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Role>> {
        Ok(self.roles.get(&id).map(|r| r.clone()))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>> {
        Ok(self.roles.iter().find(|r| r.name == name).map(|r| r.clone()))
    }

    async fn create(&self, role: &Role) -> Result<()> {
        self.roles.insert(role.id, role.clone());
        Ok(())
    }

    async fn update(&self, role: &Role) -> Result<()> {
        self.roles.insert(role.id, role.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.roles.remove(&id);
        self.role_permissions.remove(&id);
        self.role_parents.remove(&id);
        for mut entry in self.role_parents.iter_mut() {
            entry.value_mut().retain(|parent_id| *parent_id != id);
        }
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Role>> {
        Ok(self.roles.iter().map(|r| r.clone()).collect())
    }

    async fn get_role_permissions(&self, role_id: Uuid) -> Result<Vec<String>> {
        let perm_ids = self.role_permissions
            .get(&role_id)
            .map(|p| p.clone())
            .unwrap_or_default();

        let perms: Vec<String> = perm_ids
            .iter()
            .filter_map(|id| self.permissions.get(id).map(|p| p.name.clone()))
            .collect();

        Ok(perms)
    }

    async fn set_role_permissions(&self, role_id: Uuid, permission_ids: &[Uuid]) -> Result<()> {
        self.role_permissions.insert(role_id, permission_ids.to_vec());
        Ok(())
    }

    async fn list_role_permissions(&self) -> Result<Vec<(String, String)>> {
        let mut pairs = Vec::new();
        for entry in self.role_permissions.iter() {
            let role_name = match self.roles.get(&*entry.key()) {
                Some(role) => role.name.clone(),
                None => continue,
            };
            for permission_id in entry.value().iter() {
                if let Some(permission) = self.permissions.get(permission_id) {
                    pairs.push((role_name.clone(), permission.name.clone()));
                }
            }
        }
        Ok(pairs)
    }

    async fn list_role_inheritance(&self) -> Result<Vec<(String, String)>> {
        let mut pairs = Vec::new();
        for entry in self.role_parents.iter() {
            let child_name = match self.roles.get(&*entry.key()) {
                Some(role) => role.name.clone(),
                None => continue,
            };
            for parent_id in entry.value().iter() {
                if let Some(parent) = self.roles.get(parent_id) {
                    pairs.push((child_name.clone(), parent.name.clone()));
                }
            }
        }
        Ok(pairs)
    }

    async fn get_role_parents(&self, role_id: Uuid) -> Result<Vec<Role>> {
        let parent_ids = self
            .role_parents
            .get(&role_id)
            .map(|p| p.clone())
            .unwrap_or_default();

        Ok(parent_ids
            .iter()
            .filter_map(|id| self.roles.get(id).map(|r| r.clone()))
            .collect())
    }

    async fn set_role_parents(&self, role_id: Uuid, parent_ids: &[Uuid]) -> Result<()> {
        self.role_parents.insert(role_id, parent_ids.to_vec());
        Ok(())
    }
}

#[async_trait]
impl PermissionRepository for InMemoryAuthRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Permission>> {
        Ok(self.permissions.get(&id).map(|p| p.clone()))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Permission>> {
        Ok(self.permissions.iter().find(|p| p.name == name).map(|p| p.clone()))
    }

    async fn create(&self, permission: &Permission) -> Result<()> {
        self.permissions.insert(permission.id, permission.clone());
        Ok(())
    }

    async fn update(&self, permission: &Permission) -> Result<()> {
        self.permissions.insert(permission.id, permission.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.permissions.remove(&id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Permission>> {
        Ok(self.permissions.iter().map(|p| p.clone()).collect())
    }
}
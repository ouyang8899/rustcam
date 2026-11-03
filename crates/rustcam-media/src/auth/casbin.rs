use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use casbin::{ CoreApi, DefaultModel, Enforcer, MgmtApi, NullAdapter };
use tokio::sync::RwLock;

use super::errors::{AuthError, Result};
use super::repository::RoleRepository;

const DEFAULT_MODEL_PATH: &str = "casbin_model.conf";

/// Casbin 只负责模型（matcher/effect）；策略（角色 -> 权限）来自数据库。
///
/// 策略来源：
/// - `p` 规则：角色名 + 权限名拆出的 (obj, act)
/// - `g` 规则：角色继承关系（子角色 -> 父角色）
///
/// 重载是原子的：先在新的 Enforcer 上构建完整策略，成功后再整体替换，
/// 任何一步失败都保留原有内存策略。
pub struct CasbinManager {
    enforcer: Arc<RwLock<Enforcer>>,
    model_path: PathBuf,
}

impl CasbinManager {
    pub async fn new<R: RoleRepository + 'static>(role_repo: Arc<R>) -> Result<Self> {
        Self::new_with_model_path(DEFAULT_MODEL_PATH, role_repo).await
    }

    /// 指定 model 文件路径（测试用），策略仍从 role_repo 加载。
    pub async fn new_with_model_path<P, R>(model_path: P, role_repo: Arc<R>) -> Result<Self>
    where
        P: AsRef<Path>,
        R: RoleRepository + 'static,
    {
        let model_path = model_path.as_ref().to_path_buf();

        let enforcer = build_enforcer(&model_path, Vec::new(), Vec::new()).await?;

        let manager = Self {
            enforcer: Arc::new(RwLock::new(enforcer)),
            model_path,
        };

        if let Err(e) = manager.reload_from_db(role_repo.as_ref()).await {
            tracing::warn!(
                error = %e,
                "[Casbin] failed to load policies from database, starting with empty policy"
            );
        }

        Ok(manager)
    }

    /// 从数据库重新加载全部角色权限与继承关系，并原子替换内存策略。
    pub async fn reload_from_db<R: RoleRepository + ?Sized>(&self, role_repo: &R) -> Result<()> {
        let role_permissions = role_repo
            .list_role_permissions()
            .await
            .map_err(|e| AuthError::BadRequest(format!("Casbin policy load error: {}", e)))?;

        let inheritance = role_repo
            .list_role_inheritance()
            .await
            .map_err(|e| AuthError::BadRequest(format!("Casbin inheritance load error: {}", e)))?;

        let mut policies: Vec<Vec<String>> = Vec::with_capacity(role_permissions.len());
        for (role, permission) in role_permissions {
            let (obj, act) = parse_permission(&permission)?;
            policies.push(vec![role, obj, act]);
        }

        let grouping: Vec<Vec<String>> = inheritance
            .iter()
            .map(|(child, parent)| vec![child.clone(), parent.clone()])
            .collect();

        detect_role_cycle(&grouping)?;

        let enforcer = build_enforcer(&self.model_path, policies, grouping).await?;

        *self.enforcer.write().await = enforcer;

        Ok(())
    }

    pub async fn enforce(&self, role: &str, obj: &str, act: &str) -> bool {
        self.enforcer.read().await.enforce((role, obj, act)).unwrap_or(false)
    }
}

async fn build_enforcer(
    model_path: &Path,
    policies: Vec<Vec<String>>,
    grouping: Vec<Vec<String>>,
) -> Result<Enforcer> {
    let model = DefaultModel::from_file(model_path)
        .await
        .map_err(|e| AuthError::BadRequest(format!("Casbin init error: {}", e)))?;

    let mut enforcer = Enforcer::new(model, NullAdapter)
        .await
        .map_err(|e| AuthError::BadRequest(format!("Casbin init error: {}", e)))?;

    if !policies.is_empty() {
        enforcer
            .add_policies(policies)
            .await
            .map_err(|e| AuthError::BadRequest(format!("Casbin policy add error: {}", e)))?;
    }

    if !grouping.is_empty() {
        enforcer
            .add_grouping_policies(grouping)
            .await
            .map_err(|e| AuthError::BadRequest(format!("Casbin grouping add error: {}", e)))?;
        enforcer
            .build_role_links()
            .map_err(|e| AuthError::BadRequest(format!("Casbin role links error: {}", e)))?;
    }

    Ok(enforcer)
}

/// 数据库中的权限名约定为 `obj:act`；没有冒号时视为任意对象的动作。
///
/// 非法权限名（空、含空白、对象或动作为空）会让整次重载失败，
/// 避免拼写错误被静默忽略后权限被悄悄放大或缩小。
pub(crate) fn parse_permission(name: &str) -> Result<(String, String)> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err(AuthError::BadRequest("Permission name is empty".to_string()));
    }

    if trimmed.chars().any(|c| c.is_whitespace()) {
        return Err(AuthError::BadRequest(format!(
            "Permission name contains whitespace: {}",
            name
        )));
    }

    match trimmed.split_once(':') {
        Some((obj, act)) => {
            if obj.is_empty() || act.is_empty() {
                return Err(AuthError::BadRequest(format!(
                    "Permission name must be in `obj:act` form: {}",
                    name
                )));
            }
            Ok((obj.to_string(), act.to_string()))
        }
        None => Ok(("*".to_string(), trimmed.to_string())),
    }
}

/// 校验角色继承关系是否成环；成环时返回错误。
pub(crate) fn validate_role_inheritance(edges: &[(String, String)]) -> Result<()> {
    let grouping: Vec<Vec<String>> = edges
        .iter()
        .map(|(child, parent)| vec![child.clone(), parent.clone()])
        .collect();
    detect_role_cycle(&grouping)
}

/// 检测角色继承是否成环；成环时返回错误，避免角色链无限展开。
fn detect_role_cycle(grouping: &[Vec<String>]) -> Result<()> {
    let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
    for rule in grouping {
        if rule.len() < 2 {
            continue;
        }
        parents
            .entry(rule[0].as_str())
            .or_default()
            .push(rule[1].as_str());
    }

    let mut visited: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = Vec::new();

    for node in parents.keys() {
        if visited.contains(node) {
            continue;
        }
        if let Some(cycle) = find_cycle(node, &parents, &mut visited, &mut stack) {
            return Err(AuthError::BadRequest(format!(
                "Role inheritance cycle detected: {}",
                cycle.join(" -> ")
            )));
        }
    }

    Ok(())
}

fn find_cycle<'a>(
    node: &'a str,
    parents: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    stack: &mut Vec<&'a str>,
) -> Option<Vec<String>> {
    visited.insert(node);
    stack.push(node);

    if let Some(parent_list) = parents.get(node) {
        for parent in parent_list {
            if stack.contains(parent) {
                let start = stack.iter().position(|n| *n == *parent).unwrap_or(0);
                let mut cycle: Vec<String> =
                    stack[start..].iter().map(|n| (*n).to_string()).collect();
                cycle.push((*parent).to_string());
                return Some(cycle);
            }
            if !visited.contains(parent) {
                if let Some(cycle) = find_cycle(parent, parents, visited, stack) {
                    return Some(cycle);
                }
            }
        }
    }

    stack.pop();
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::models::{Permission, Role};
    use crate::auth::repository::{InMemoryAuthRepository, PermissionRepository};

    fn model_path() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../casbin_model.conf").to_string()
    }

    async fn seed(
        repo: &Arc<InMemoryAuthRepository>,
        role_name: &str,
        permission_name: &str,
    ) -> (Role, Permission) {
        let role = Role::new(role_name.into(), None);
        let permission = Permission::new(permission_name.into(), None);

        RoleRepository::create(repo.as_ref(), &role).await.unwrap();
        PermissionRepository::create(repo.as_ref(), &permission).await.unwrap();
        RoleRepository::set_role_permissions(repo.as_ref(), role.id, &[permission.id])
            .await
            .unwrap();

        (role, permission)
    }

    #[tokio::test]
    async fn policies_are_loaded_from_database() {
        let repo = Arc::new(InMemoryAuthRepository::new());

        let (role, permission) = seed(&repo, "Admin", "device:read").await;
        let _ = (role, permission);

        let casbin = CasbinManager::new_with_model_path(model_path(), repo)
            .await
            .unwrap();

        assert!(casbin.enforce("Admin", "device", "read").await);
        assert!(!casbin.enforce("Admin", "device", "write").await);
        assert!(!casbin.enforce("Viewer", "device", "read").await);
    }

    #[tokio::test]
    async fn child_role_inherits_parent_permissions() {
        let repo = Arc::new(InMemoryAuthRepository::new());

        let (admin, _) = seed(&repo, "Admin", "drone:control:acquire").await;
        let operator = Role::new("Operator".into(), None);
        RoleRepository::create(repo.as_ref(), &operator).await.unwrap();
        RoleRepository::set_role_parents(repo.as_ref(), operator.id, &[admin.id])
            .await
            .unwrap();

        let casbin = CasbinManager::new_with_model_path(model_path(), repo.clone())
            .await
            .unwrap();

        assert!(casbin.enforce("Operator", "drone", "control:acquire").await);
        assert!(casbin.enforce("Admin", "drone", "control:acquire").await);
    }

    #[tokio::test]
    async fn inheritance_is_transitive() {
        let repo = Arc::new(InMemoryAuthRepository::new());

        let (admin, _) = seed(&repo, "Admin", "device:read").await;
        let operator = Role::new("Operator".into(), None);
        let viewer = Role::new("Viewer".into(), None);
        RoleRepository::create(repo.as_ref(), &operator).await.unwrap();
        RoleRepository::create(repo.as_ref(), &viewer).await.unwrap();
        RoleRepository::set_role_parents(repo.as_ref(), operator.id, &[admin.id])
            .await
            .unwrap();
        RoleRepository::set_role_parents(repo.as_ref(), viewer.id, &[operator.id])
            .await
            .unwrap();

        let casbin = CasbinManager::new_with_model_path(model_path(), repo)
            .await
            .unwrap();

        assert!(casbin.enforce("Viewer", "device", "read").await);
    }

    #[tokio::test]
    async fn inheritance_cycle_is_rejected_and_old_policy_is_kept() {
        let repo = Arc::new(InMemoryAuthRepository::new());

        let (admin, _) = seed(&repo, "Admin", "device:read").await;
        let operator = Role::new("Operator".into(), None);
        RoleRepository::create(repo.as_ref(), &operator).await.unwrap();

        let casbin = CasbinManager::new_with_model_path(model_path(), repo.clone())
            .await
            .unwrap();

        RoleRepository::set_role_parents(repo.as_ref(), operator.id, &[admin.id])
            .await
            .unwrap();
        RoleRepository::set_role_parents(repo.as_ref(), admin.id, &[operator.id])
            .await
            .unwrap();

        assert!(casbin.reload_from_db(repo.as_ref()).await.is_err());
        assert!(casbin.enforce("Admin", "device", "read").await);
    }

    #[tokio::test]
    async fn invalid_permission_name_rejects_reload() {
        let repo = Arc::new(InMemoryAuthRepository::new());

        let (role, _) = seed(&repo, "Admin", "device:read").await;

        let casbin = CasbinManager::new_with_model_path(model_path(), repo.clone())
            .await
            .unwrap();

        let broken = Permission::new("bad name".into(), None);
        PermissionRepository::create(repo.as_ref(), &broken).await.unwrap();
        RoleRepository::set_role_permissions(repo.as_ref(), role.id, &[broken.id])
            .await
            .unwrap();

        assert!(casbin.reload_from_db(repo.as_ref()).await.is_err());
        assert!(casbin.enforce("Admin", "device", "read").await);
    }

    #[test]
    fn permission_name_is_split_into_object_and_action() {
        assert_eq!(
            parse_permission("device:read").unwrap(),
            ("device".to_string(), "read".to_string())
        );
        assert_eq!(
            parse_permission("read").unwrap(),
            ("*".to_string(), "read".to_string())
        );
        assert!(parse_permission("").is_err());
        assert!(parse_permission("bad name").is_err());
        assert!(parse_permission("device:").is_err());
        assert!(parse_permission(":read").is_err());
    }
}

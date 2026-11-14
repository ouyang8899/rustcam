use async_trait::async_trait;
use uuid::Uuid;

use crate::error::Result;
use super::models::{NewSession, UserSession, UserSessionWithUser};

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(&self, session: &NewSession) -> Result<()>;
    async fn find_active(&self, id: Uuid) -> Result<Option<UserSession>>;
    async fn list_active_for_user(&self, user_id: Uuid) -> Result<Vec<UserSession>>;
    /// 分页查询某用户的活跃会话，按最近活跃时间倒序。
    async fn list_active_for_user_paged(
        &self,
        user_id: Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<UserSession>, i64)>;
    /// 分页查询全部活跃会话（带用户名），按最近活跃时间倒序。
    async fn list_all_active_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<UserSessionWithUser>, i64)>;
    async fn touch(&self, id: Uuid) -> Result<()>;
    async fn revoke(&self, id: Uuid) -> Result<bool>;
}

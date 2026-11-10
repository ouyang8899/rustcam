use std::sync::Arc;

use crate::domain::video_tour::{
    validate_tour_scheme, CreateTourSchemeRequest, TourScheme, UpdateTourSchemeRequest,
};
use crate::error::{AppError, Result};
use crate::infrastructure::db_repository::DbRepository;

/// 视频轮询方案服务：只负责方案的存取与校验，不维护任何任务状态。
pub struct TourSchemeService {
    repo: Arc<DbRepository>,
}

impl TourSchemeService {
    pub fn new(repo: Arc<DbRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<TourScheme>> {
        Ok(self.repo.list_tour_schemes().await)
    }

    pub async fn get(&self, id: i64) -> Result<Option<TourScheme>> {
        Ok(self.repo.get_tour_scheme(id).await)
    }

    /// 取布局窗口数与窗口 id 列表：W = layout_json 元素个数，窗口 id 取 layout_json 的 `id`。
    async fn layout_meta(&self, layout_id: i32) -> Result<(usize, Vec<String>)> {
        let layout = self
            .repo
            .get_layout(layout_id as i64)
            .await
            .ok_or_else(|| AppError::BadRequest(format!("布局 {} 不存在", layout_id)))?;
        if layout.layout_json.is_empty() {
            return Err(AppError::BadRequest(format!(
                "布局 {} 没有可用窗口",
                layout_id
            )));
        }
        Ok((
            layout.layout_json.len(),
            layout.layout_json.into_iter().map(|item| item.id).collect(),
        ))
    }

    pub async fn create(&self, req: CreateTourSchemeRequest) -> Result<TourScheme> {
        let (layout_w, layout_item_ids) = self.layout_meta(req.layout_id).await?;
        validate_tour_scheme(&req, layout_w, &layout_item_ids).map_err(AppError::BadRequest)?;
        let id = self.repo.create_tour_scheme(&req).await?;
        self.get(id)
            .await?
            .ok_or_else(|| AppError::Internal("方案创建后读取失败".to_string()))
    }

    pub async fn update(&self, id: i64, req: UpdateTourSchemeRequest) -> Result<()> {
        let current = self
            .get(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("轮询方案 {} 不存在", id)))?;
        let (layout_w, layout_item_ids) = self.layout_meta(req.scheme.layout_id).await?;
        validate_tour_scheme(&req.scheme, layout_w, &layout_item_ids).map_err(AppError::BadRequest)?;
        // 布局变更时旧锁定已失效，服务端清空锁定集合。
        let clear_locks = current.layout_id != req.scheme.layout_id;
        let updated = self
            .repo
            .update_tour_scheme(id, &req.scheme, req.expected_version, clear_locks)
            .await?;
        if !updated {
            return Err(AppError::Conflict(
                "方案已被其他客户端更新，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        if !self.repo.delete_tour_scheme(id).await? {
            return Err(AppError::NotFound(format!("轮询方案 {} 不存在", id)));
        }
        Ok(())
    }
}

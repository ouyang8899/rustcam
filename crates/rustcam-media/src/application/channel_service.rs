use std::sync::Arc;
use crate::domain::{ Channel, CreateChannelRequest, UpdateChannelRequest };
use crate::error::Result;

pub struct ChannelService {
    repo: Arc<crate::infrastructure::DbRepository>,
}

impl ChannelService {
    pub fn new(repo: Arc<crate::infrastructure::DbRepository>) -> Self {
        Self { repo }
    }

    pub async fn get_channel(&self, device_tag: &str, channel_tag: &str) -> Result<Option<Channel>> {
        self.repo.get_channel(device_tag, channel_tag).await
    }

    pub async fn list_channels(&self) -> Result<Vec<Channel>> {
        self.repo.list_all_channels().await
    }

    pub fn list_channels_cached(&self) -> Vec<Channel> {
        self.repo.list_all_channels_cached()
    }

    pub fn list_channels_paginated(&self, limit: usize, offset: usize) -> Vec<Channel> {
        let all = self.repo.list_all_channels_cached();
        all.into_iter().skip(offset).take(limit).collect()
    }

    pub async fn list_channels_paginated_db(
        &self,
        device_tag: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Channel>> {
        self.repo.list_channels_paginated(device_tag, search, status, limit, offset).await
    }

    pub fn count_channels(&self) -> usize {
        self.repo.list_all_channels_cached().len()
    }

    pub async fn count_channels_db(
        &self,
        device_tag: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
    ) -> Result<usize> {
        self.repo.count_channels(device_tag, search, status).await
    }

    pub fn get_channels_by_device(&self, device_tag: &str) -> Vec<Channel> {
        self.repo.get_channels_by_device_tag_cached(device_tag)
    }

    pub async fn create_channel(&self, channel: &Channel) -> Result<i64> {
        let id = self.repo.create_channel(channel).await?;
        Ok(id)
    }

    pub async fn update_channel(&self, channel: &Channel) -> Result<()> {
        self.repo.update_channel(channel).await
    }

    pub async fn get_or_create_channel(
        &self,
        device_tag: &str,
        channel_tag: &str,
        name: &str,
    ) -> Result<Channel> {
        if let Some(channel) = self.repo.get_channel(device_tag, channel_tag).await? {
            return Ok(channel);
        }

        let channel = Channel::new(
            device_tag.to_string(),
            channel_tag.to_string(),
            name.to_string(),
        );
        let id = self.repo.create_channel(&channel).await?;
        
        let mut created = channel;
        created.id = id;
        Ok(created)
    }

    /// 更新通道信息。
    ///
    /// 描述性字段走 `update_channel`，坐标走 `update_channel_position`，
    /// 两者互不干扰，因此注册流程刷新描述性字段不会动坐标。
    pub async fn update_channel_info(
        &self,
        device_tag: &str,
        channel_tag: &str,
        req: UpdateChannelRequest,
    ) -> Result<Channel> {
        let mut channel = self
            .repo
            .get_channel(device_tag, channel_tag)
            .await?
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!(
                    "Channel {}/{} not found",
                    device_tag, channel_tag
                ))
            })?;

        req.apply_to(&mut channel);

        if let Some((latitude, longitude)) =
            req.effective_position(channel.latitude, channel.longitude)
        {
            self.repo
                .update_channel_position(device_tag, channel_tag, latitude, longitude)
                .await?;
            channel.latitude = latitude;
            channel.longitude = longitude;
        }

        self.repo.update_channel(&channel).await?;
        Ok(channel)
    }

    /// 手工创建通道。
    pub async fn create_channel_info(&self, req: CreateChannelRequest) -> Result<Channel> {
        // 落库时 repository 会把 tag 转小写，这里先转一遍，
        // 保证返回的对象跟之后 GET 读到的缓存对象一致。
        let mut channel = Channel::new(
            req.device_tag.to_lowercase(),
            req.channel_tag.to_lowercase(),
            req.name.clone(),
        );
        channel.civil_code = req.civil_code;
        channel.address = req.address;
        channel.ip_address = req.ip_address;
        channel.port = req.port.unwrap_or(0);
        channel.manufacturer = req.manufacturer;
        channel.model = req.model;
        channel.latitude = req.latitude;
        channel.longitude = req.longitude;

        let id = self.repo.create_channel(&channel).await?;
        channel.id = id;
        Ok(channel)
    }
}

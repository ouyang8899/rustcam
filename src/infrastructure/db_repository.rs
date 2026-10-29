use std::sync::Arc;
use dashmap::DashMap;

pub const DEFAULT_MAX_CONNECTIONS: u64 = 50;
  use rbatis::rbdc::datetime::DateTime as RbatisDateTime;
  use rbatis::rbdc::db::Driver as RbdcDriver;
  use rbatis::RBatis;
use chrono::Utc;
use crate::common::device_utils::{stream_key_to_cache_key,format_cache_key,format_cache_key_opt};
use crate::domain::server::Server;
use crate::domain::{Alarm, CreateAlarmRequest, Channel, Device, DeviceStatus, Protocol, Recording, Session, Stream};
use crate::domain::device_group::{CreateGroupRequest, DeviceGroup, DeviceGroupNode, UpdateGroupRequest};
use crate::domain::ptz::PtzPreset;
use crate::domain::region::{Region, RegionNode};
use crate::domain::player_layout::PlayerLayout;
use crate::domain::map_settings::MapSettings;
use crate::error::AppError;
use crate::error::Result;
use crate::sql_mappers::{
    device_select_all, server_select_all, session_select_all, session_select_paginated, session_count, stream_select_all, stream_select_paginated, stream_count,
    recording_select_paginated, recording_count, recording_select_by_id, recording_select_by_device_tag, recording_select_active, recording_select_active_by_stream, device_insert, device_update,
    device_delete_by_device_tag, device_select_by_parent_device_tag,
    device_select_paginated, device_count,
    group_select_all, group_insert, group_update, group_delete,
    region_select_all,
    server_insert, server_update, server_delete_by_id, server_select_by_tag,
    session_insert, session_delete_by_id, session_update_state, session_update_states_by_stream, session_delete_unfinished,
    stream_insert, stream_update, stream_select_by_token, stream_delete_duplicates,
    stream_reset_state, stream_delete_others,
    recording_insert, recording_update, recording_delete_by_id, recording_delete_stale_errors, recording_update_start_result,
    recording_delete_segments_by_recording_id,
    recording_select_pending, recording_select_pending_by_stream,
    alarm_insert, alarm_select_by_device, alarm_select_all, alarm_select_by_id,
    alarm_select_filtered, alarm_count_filtered,
    alarm_count_by_device, alarm_count_all, alarm_mark_processed,
    channel_insert, channel_update, channel_select_all, channel_select_paginated, channel_count,
    channel_select_by_device_tag, channel_select_by_device_tag_and_channel_tag,
    channel_delete_by_device_tag_and_channel_tag,
    ServerRow, GroupRow, SessionRow,
    layout_select_by_id, layout_select_all, layout_select_default,
    layout_insert, layout_update, layout_set_default, layout_delete,
    map_settings_select, map_settings_upsert,
    policy_insert, policy_update, policy_delete_by_id, policy_select_all, policy_select_by_id,
    policy_select_by_device_tag, policy_select_by_alarm_triggered,
    policy_select_paginated, policy_count,
    time_window_insert, time_window_delete_by_policy_id, time_window_select_by_policy_id,
    segment_insert,  segment_select_by_id, segment_select_paginated, segment_select_by_recording_id,
    segment_count_by_device_channel_time, segment_select_filtered_paginated,
    segment_count_filtered,
};
use rbatis::RBatisRef;

pub struct DbRepository {
    rb: RBatis,
    pub devices_cache: Arc<DashMap<String, Device>>,

    pub channels_cache: Arc<DashMap<String, Channel>>,
    pub servers_cache: Arc<DashMap<i64, ServerRow>>,
    pub sessions_cache: Arc<DashMap<i64, Session>>,

    pub streams_cache: Arc<DashMap<String, Stream>>,

    pub groups_cache: Arc<DashMap<i64, DeviceGroup>>,

    pub regions_cache: Arc<DashMap<String, Region>>,
}



impl DbRepository {

    async fn init_pool(rb: &RBatis, url: &str, max_connections: u64) -> anyhow::Result<()> {
        let driver = rbdc_pg::driver::PgDriver {};
        let mut option = driver.default_option();
        option.set_uri(url)
            .map_err(|e| anyhow::anyhow!("DB option failed: {}", e))?;

        let pool = crate::infrastructure::bb8_pool::Bb8Pool::new(
            Arc::new(Box::new(driver)),
            Arc::new(option),
            max_connections.max(4) as u32,
            std::time::Duration::from_secs(10),
        )
        .await
        .map_err(|e| anyhow::anyhow!("DB pool init failed: {}", e))?;

        rb.init_pool(pool)
            .map_err(|e| anyhow::anyhow!("DB pool init failed: {}", e))?;

        Ok(())
    }

    async fn ensure_schema(&self) {
        let statements = [
            // 通道侧地理结构：坐标下沉到通道，geom 由触发器维护。
            // CREATE EXTENSION 必须排在 ADD COLUMN geom 之前，否则几何类型不存在。
            "CREATE EXTENSION IF NOT EXISTS postgis WITH SCHEMA public",
            "ALTER TABLE channels ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION",
            "ALTER TABLE channels ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION",
            "ALTER TABLE channels ADD COLUMN IF NOT EXISTS geom geometry(Point,4326)",
            "CREATE OR REPLACE FUNCTION channels_sync_geom() RETURNS trigger AS $$ BEGIN IF NEW.latitude IS NOT NULL AND NEW.longitude IS NOT NULL THEN NEW.geom := ST_SetSRID(ST_MakePoint(NEW.longitude, NEW.latitude), 4326); ELSE NEW.geom := NULL; END IF; RETURN NEW; END; $$ LANGUAGE plpgsql",
            "DROP TRIGGER IF EXISTS trg_channels_sync_geom ON channels",
            "CREATE TRIGGER trg_channels_sync_geom BEFORE INSERT OR UPDATE OF latitude, longitude ON channels FOR EACH ROW EXECUTE FUNCTION channels_sync_geom()",
            "CREATE INDEX IF NOT EXISTS idx_channels_geom ON channels USING GIST (geom)",
            "ALTER TABLE recordings ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE recordings ADD COLUMN IF NOT EXISTS next_retry_at TIMESTAMPTZ",
            "ALTER TABLE recordings ADD COLUMN IF NOT EXISTS max_duration_secs INTEGER",
            "ALTER TABLE recordings ADD COLUMN IF NOT EXISTS recording_type VARCHAR(32) NOT NULL DEFAULT 'timed'",
            "ALTER TABLE recordings ADD COLUMN IF NOT EXISTS alarm_id BIGINT",
            "ALTER TABLE alarms ADD COLUMN IF NOT EXISTS channel_tag VARCHAR(64)",
            "ALTER TABLE sessions ADD COLUMN IF NOT EXISTS tcp_id VARCHAR(64)",
            "CREATE INDEX IF NOT EXISTS idx_recordings_next_retry ON recordings(state, next_retry_at)",
            "CREATE INDEX IF NOT EXISTS idx_recordings_recording_type ON recordings(device_tag, channel_tag, recording_type)",
            "CREATE INDEX IF NOT EXISTS idx_alarms_channel_time ON alarms(channel_tag, alarm_time DESC)",
        ];
        for sql in statements {
            if let Err(e) = self.rb.exec(sql, vec![]).await {
                tracing::warn!("[DbRepository] ensure_schema warning: {} ({})", e, sql);
            }
        }
    }

    pub async fn new(url: &str, debug_sql: bool) -> anyhow::Result<Self> {
        Self::new_with_pool(url, debug_sql, DEFAULT_MAX_CONNECTIONS).await
    }

    pub async fn new_with_pool(
        url: &str,
        debug_sql: bool,
        max_connections: u64,
    ) -> anyhow::Result<Self> {
        let rb = RBatis::new();
        Self::init_pool(&rb, url, max_connections).await?;
        if debug_sql {
        }
        let repo = Self {
            rb,
            devices_cache: Arc::new(DashMap::new()),
            channels_cache: Arc::new(DashMap::new()),
            servers_cache: Arc::new(DashMap::new()),
            sessions_cache: Arc::new(DashMap::new()),
            streams_cache: Arc::new(DashMap::new()),
            groups_cache: Arc::new(DashMap::new()),
            regions_cache: Arc::new(DashMap::new()),
        };
        repo.ensure_schema().await;
        repo.load_caches().await?;
        Ok(repo)
    }

    pub async fn new_without_load(url: &str, debug_sql: bool) -> anyhow::Result<Self> {
        Self::new_without_load_with_pool(url, debug_sql, DEFAULT_MAX_CONNECTIONS).await
    }

    pub async fn new_without_load_with_pool(
        url: &str,
        debug_sql: bool,
        max_connections: u64,
    ) -> anyhow::Result<Self> {
        let rb = RBatis::new();
        Self::init_pool(&rb, url, max_connections).await?;
        if debug_sql {
        }
        let repo = Self {
            rb,
            devices_cache: Arc::new(DashMap::new()),
            channels_cache: Arc::new(DashMap::new()),
            servers_cache: Arc::new(DashMap::new()),
            sessions_cache: Arc::new(DashMap::new()),
            streams_cache: Arc::new(DashMap::new()),
            groups_cache: Arc::new(DashMap::new()),
            regions_cache: Arc::new(DashMap::new()),
        };
        repo.ensure_schema().await;
        Ok(repo)
    }

    pub async fn load_caches(&self) -> anyhow::Result<()> {
        self.cleanup_stale_runtime_state().await;

        let devices = device_select_all(&self.rb)
            .await
            .map_err(|e| anyhow::anyhow!("load devices: {}", e))?;
        for row in devices {
            if let Ok(device) = Device::try_from(row) {
                if let Some(ref tag) = device.device_tag {
                    self.devices_cache.insert(tag.clone(), device.clone());
                }
            }
        }

        let channels = channel_select_all(&self.rb)
            .await
            .map_err(|e| anyhow::anyhow!("load channels: {}", e))?;
        for row in channels {
            if let Ok(channel) = Channel::try_from(row) {
                let key = format_cache_key(&channel.device_tag, &channel.channel_tag);
                self.channels_cache.insert(key, channel);
            }
        }

        let servers = server_select_all(&self.rb)
            .await
            .map_err(|e| anyhow::anyhow!("load servers: {}", e))?;
        for server in servers {
            self.servers_cache.insert(server.id, server);
        }

        let sessions = session_select_all(&self.rb)
            .await
            .map_err(|e| anyhow::anyhow!("load sessions: {}", e))?;
        for row in sessions {
            if let Ok(session) = Session::try_from(row) {
                self.sessions_cache.insert(session.id, session);
            }
        }

        let streams = stream_select_all(&self.rb)
            .await
            .map_err(|e| anyhow::anyhow!("load streams: {}", e))?;
        for row in streams {
            if let Ok(stream) = Stream::try_from(row) {
                let key = format_cache_key_opt(stream.device_tag.as_deref(), stream.channel_tag.as_deref());
                self.streams_cache.insert(key, stream);
            }
        }

        self.load_groups_cache().await;
        self.load_regions_cache().await;

        self.recalculate_device_stats();
        Ok(())
    }

    async fn cleanup_stale_runtime_state(&self) {
        if let Err(e) = stream_delete_duplicates(&self.rb).await {
            tracing::error!("cleanup stale streams: {}", e);
        }
        if let Err(e) = stream_reset_state(&self.rb).await {
            tracing::error!("reset stale stream states: {}", e);
        }
        if let Err(e) = session_delete_unfinished(&self.rb).await {
            tracing::error!("cleanup stale sessions: {}", e);
        }
        if let Err(e) = recording_delete_stale_errors(&self.rb).await {
            tracing::error!("cleanup stale recordings: {}", e);
        }
    }

    async fn load_groups_cache(&self) {        match group_select_all(&self.rb).await {
            Ok(rows) => {
                for row in rows {
                    if let Ok(group) = DeviceGroup::try_from(row) {
                        self.groups_cache.insert(group.id, group);
                    }
                }
            }
            Err(e) => tracing::error!("load_groups_cache: {}", e),
        }
    }

    async fn load_regions_cache(&self) {
        match region_select_all(&self.rb).await {
            Ok(rows) => {
                for row in rows {
                    let region = row.to_region();
                    self.regions_cache.insert(region.code.clone(), region);
                }
            }
            Err(e) => tracing::error!("load_regions_cache: {}", e),
        }
    }

    pub async fn reload_devices_cache(&self) -> anyhow::Result<usize> {
        let devices = device_select_all(&self.rb)
            .await
            .map_err(|e| anyhow::anyhow!("reload devices: {}", e))?;
        let _old_len = self.devices_cache.len();
        self.devices_cache.clear();
        let mut count = 0;
        for row in devices {
            if let Ok(device) = Device::try_from(row) {
                if let Some(ref tag) = device.device_tag {
                    self.devices_cache.insert(tag.clone(), device);
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    pub fn recalculate_device_stats(&self) {
        let mut region_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let group_counts: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();

        for device in self.devices_cache.iter() {
            let device = device.value();
            if device.parent_device_tag.is_some() {
                continue;
            }

            if let Some(ref device_region) = device.region_code {
                if !device_region.is_empty() {

                    if let Some(count) = region_counts.get_mut(device_region) {
                        *count += 1;
                    } else {
                        region_counts.insert(device_region.clone(), 1);
                    }
                }
            }
        }

        for (code, count) in region_counts {
            if let Some(mut region) = self.regions_cache.get_mut(&code) {
                region.device_count = count;

            }
        }

        for (id, count) in group_counts {
            if let Some(mut group) = self.groups_cache.get_mut(&id) {
                group.device_count = count;
            }
        }

    }

    pub fn rb(&self) -> &RBatis {
        &self.rb
    }

    pub fn devices_cache(&self) -> &Arc<DashMap<String, Device>> {
        &self.devices_cache
    }

    pub fn channels_cache(&self) -> &Arc<DashMap<String, Channel>> {
        &self.channels_cache
    }

    pub fn device_cached(&self, device_tag: &str) -> Option<Device> {
        self.devices_cache.get(device_tag).map(|d| d.clone())
    }

    pub fn stream_cached(&self, cache_key: &str) -> Option<Stream> {
        self.streams_cache.get(cache_key).map(|s| s.clone())
    }

    pub fn channel_cached(&self, cache_key: &str) -> Option<Channel> {
        self.channels_cache.get(cache_key).map(|c| c.clone())
    }

    pub async fn create_device(&self, device: &Device) -> Result<i64> {
        let device_tag = device.device_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let parent_device_tag = device.parent_device_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());

        let rtsp_url = device.extended.as_ref()
            .and_then(|e| e.get("rtsp_full_url"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let gb_username = device.extended.as_ref()
            .and_then(|e| e.get("gb_username"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let gb_password = device.extended.as_ref()
            .and_then(|e| e.get("gb_password"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let ssrc = device.extended.as_ref()
            .and_then(|e| e.get("ssrc"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut extended_for_db = device.extended.clone().unwrap_or(serde_json::Value::Object(Default::default()));
        if let Some(stream_config) = &device.stream_config {
            if let Some(obj) = extended_for_db.as_object_mut() {
                obj.insert("stream_config".to_string(), serde_json::json!(stream_config));
            }
        }

        let device_type_str = device.device_type.to_string();

        // Devices without coordinates are spread around the default position
        // so that they can be told apart on the map.
        let (fallback_lat, fallback_lng) = crate::domain::device::fallback_map_position(
            device.device_tag.as_deref().unwrap_or(&device.name),
        );
        let latitude = device.latitude.or(Some(fallback_lat));
        let longitude = device.longitude.or(Some(fallback_lng));

        let _result = device_insert(
            &self.rb,
            Some(&device.name),
            Some(&protocol_to_str(&device.protocol)),
            Some(&status_to_str(&device.status)),
            Some(&device.host),
            Some(device.port as i32),
            rtsp_url.as_deref(),
            device_tag.as_deref(),
            parent_device_tag.as_deref(),
            Some(device.is_public),
            Some(RbatisDateTime::from_timestamp_millis(device.created_at.timestamp_millis())),
            device.device_username.as_deref(),
            device.device_password.as_deref(),
            device.playback_username.as_deref(),
            device.playback_password.as_deref(),
            device.media_server_tag.as_deref(),
            serde_json::to_value(&device.push_urls).ok(),
            serde_json::to_value(&device.pull_urls).ok(),
            device.region_code.as_deref(),
            latitude,
            longitude,
            Some(RbatisDateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis())),
            ssrc.as_deref(),
            gb_username.as_deref(),
            gb_password.as_deref(),
            Some(extended_for_db.clone()),
            device.group_id,
            device.app.as_deref(),
            Some(&device_type_str),
            device.device_type_code.as_deref(),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        if let Some(ref tag) = device_tag {
            if let Ok(Some(db_device)) = self.fetch_device_by_tag(tag).await {
                self.devices_cache.insert(tag.clone(), db_device.clone());
                self.recalculate_device_stats();
                return Ok(db_device.id);
            }
        }
        self.recalculate_device_stats();
        Err(AppError::Internal("Failed to retrieve created device".to_string()))
    }

    pub async fn fetch_device_by_tag(&self, device_tag: &str) -> Result<Option<Device>> {
        let rows: Vec<crate::sql_mappers::DeviceRow> = crate::sql_mappers::device_select_by_device_tag(&self.rb, device_tag).await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().next().and_then(|r| Device::try_from(r).ok()))
    }

    pub async fn get_device(&self, device_tag: &str) -> Result<Option<Device>> {
        Ok(self.devices_cache.get(device_tag).map(|r| r.clone()))
    }

    pub async fn get_device_by_id(&self, id: i64) -> Result<Option<Device>> {
        let rows: Vec<crate::sql_mappers::DeviceRow> = crate::sql_mappers::device_select_by_id(&self.rb, id).await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().next().and_then(|r| Device::try_from(r).ok()))
    }

    pub fn get_device_by_device_tag(&self, device_tag: &str) -> Option<Device> {
        self.devices_cache.get(device_tag).map(|r| r.clone())
    }

    pub fn get_device_by_stream_key(&self, stream_key: &str) -> Option<Device> {
        let cache_key = self.get_stream_cache_key_by_stream_key(stream_key)?;
        let stream = self.streams_cache.get(&cache_key)?;
        let device_tag = stream.device_tag.as_deref()?;
        self.get_device_by_device_tag(device_tag)
    }

    pub async fn get_device_by_protocol_and_host(&self, protocol: &Protocol, host: &str) -> Option<Device> {
        self.devices_cache.iter()
            .find(|d| d.protocol == *protocol && d.host == host)
            .map(|r| r.clone())
    }

    pub async fn list_devices(&self) -> Vec<Device> {
        self.devices_cache.iter()
            .map(|d| {
                let mut device = d.clone();
                if let Some(ref tag) = device.device_tag {
                    device.has_stream = self.device_has_active_stream(tag);
                }
                device
            })
            .collect()
    }

    pub async fn list_devices_paginated(&self, limit: usize, offset: usize, search: Option<&str>, region_code: Option<&str>, protocol: Option<&str>, status: Option<&str>, device_type: Option<&str>, group_id: Option<i64>) -> anyhow::Result<Vec<Device>> {
        let search_pattern = search.map(|s| format!("%{}%", s.to_lowercase()));
        let rows = device_select_paginated(&self.rb, search_pattern.as_deref(), region_code, protocol, status, device_type, group_id, limit as i64, offset as i64).await.map_err(|e| anyhow::anyhow!("query devices: {}", e))?;
        let mut devices: Vec<Device> = rows.into_iter().filter_map(|row| Device::try_from(row).ok()).collect();

        for device in &mut devices {
            if let Some(ref tag) = device.device_tag {
                if let Some(cache_device) = self.devices_cache.get(tag) {
                    device.is_online = cache_device.is_online;
                }
                device.has_stream = self.device_has_active_stream(tag);
            }
        }
        Ok(devices)
    }

    pub fn device_has_active_stream(&self, device_tag: &str) -> bool {
        self.streams_cache.iter()
            .any(|s| s.device_tag.as_deref() == Some(device_tag) && s.state.is_alive())
    }

    pub async fn count_devices_filtered(&self, search: Option<&str>, region_code: Option<&str>, protocol: Option<&str>, status: Option<&str>, device_type: Option<&str>, group_id: Option<i64>) -> anyhow::Result<usize> {
        let search_pattern = search.map(|s| format!("%{}%", s.to_lowercase()));
        let count = device_count(&self.rb, search_pattern.as_deref(), region_code, protocol, status, device_type, group_id).await.map_err(|e| anyhow::anyhow!("count devices: {}", e))?;
        Ok(count as usize)
    }

    pub async fn count_devices(&self) -> usize {
        self.devices_cache.len()
    }

    pub async fn count_devices_top_level(&self) -> usize {
        self.devices_cache.iter()
            .filter(|d| d.parent_device_tag.is_none())
            .count()
    }

    pub async fn list_online_devices_paginated(&self, limit: usize, offset: usize) -> Vec<Device> {
        self.devices_cache.iter()
            .filter(|d| d.status == DeviceStatus::Online)
            .skip(offset)
            .take(limit)
            .map(|r| r.clone())
            .collect()
    }

    pub async fn count_online_devices(&self) -> usize {
        self.devices_cache.iter()
            .filter(|d| d.status == DeviceStatus::Online)
            .count()
    }

    pub async fn update_device(&self, device: &Device) -> Result<()> {
        let device_tag = device.device_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let parent_device_tag = device.parent_device_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());

        let mut extended_json = device.extended.clone().unwrap_or(serde_json::Value::Object(Default::default()));
        if let Some(stream_config) = &device.stream_config {
            if let Some(obj) = extended_json.as_object_mut() {
                obj.insert("stream_config".to_string(), serde_json::json!(stream_config));
            }
        }
        let push_urls_json = serde_json::to_value(&device.push_urls).ok();
        let pull_urls_json = serde_json::to_value(&device.pull_urls).ok();

        let rtsp_url = extended_json.get("rtsp_full_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let gb_username = extended_json.get("gb_username")
            .and_then(|v| v.as_str())
            .map(String::from);
        let gb_password = extended_json.get("gb_password")
            .and_then(|v| v.as_str())
            .map(String::from);
        let ssrc = extended_json.get("ssrc")
            .and_then(|v| v.as_str())
            .map(String::from);

        let device_type_str = device.device_type.to_string();

        // The device carries the stored coordinates; devices without them get
        // a spread position around the default position.
        let (fallback_lat, fallback_lng) = crate::domain::device::fallback_map_position(
            device.device_tag.as_deref().unwrap_or(&device.name),
        );
        let latitude = device.latitude.or(Some(fallback_lat));
        let longitude = device.longitude.or(Some(fallback_lng));

        device_update(
            &self.rb,
            device.id,
            Some(&device.name),
            Some(&protocol_to_str(&device.protocol)),
            Some(&status_to_str(&device.status)),
            Some(&device.host),
            Some(device.port as i32),
            rtsp_url.as_deref(),
            device_tag.as_deref(),
            parent_device_tag.as_deref(),
            Some(device.is_public),
            device.device_username.as_deref(),
            device.device_password.as_deref(),
            device.playback_username.as_deref(),
            device.playback_password.as_deref(),
            device.media_server_tag.as_deref(),
            push_urls_json,
            pull_urls_json,
            device.region_code.as_deref(),
            latitude,
            longitude,
            Some(RbatisDateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis())),
            ssrc.as_deref(),
            gb_username.as_deref(),
            gb_password.as_deref(),
            Some(extended_json),
            device.group_id,
            device.app.as_deref(),
            Some(&device_type_str),
            device.device_type_code.as_deref(),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        if let Some(ref tag) = device_tag {
            let mut dev = device.clone();
            dev.device_tag = device_tag.clone();
            dev.parent_device_tag = parent_device_tag;
            self.devices_cache.insert(tag.clone(), dev);
        }
        self.recalculate_device_stats();
        Ok(())
    }

    pub async fn delete_device(&self, device_tag: &str) -> Result<()> {
        device_delete_by_device_tag(&self.rb, device_tag)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        self.devices_cache.remove(device_tag);
        self.recalculate_device_stats();
        Ok(())
    }

    pub async fn get_children_by_parent_tag(&self, parent_tag: &str) -> Result<Vec<Device>> {
        let rows = device_select_by_parent_device_tag(&self.rb, parent_tag)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Device::try_from(r).ok()).collect())
    }

    pub async fn update_device_group(&self, device_id: i64, group_id: Option<i64>) -> Result<()> {
        if let Some(mut device) = self.get_device_by_id(device_id).await? {
            device.group_id = group_id;
            self.update_device(&device).await?;
        }
        Ok(())
    }

    pub async fn create_channel(&self, channel: &Channel) -> Result<i64> {
        let device_tag = if channel.device_tag.is_empty() { None } else { Some(channel.device_tag.to_lowercase()) };
        let channel_tag = if channel.channel_tag.is_empty() { None } else { Some(channel.channel_tag.to_lowercase()) };
        let parent_channel_tag = channel.parent_channel_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let status_str = channel.status.to_string();
        let device_type_str = channel.device_type.to_string();
        let channel_extended_json = channel.channel_extended.clone();
        let extended_json = channel.extended.clone().unwrap_or(serde_json::Value::Object(Default::default()));
        let mut channel_clone = channel.clone();
        channel_clone.device_tag = device_tag.clone().unwrap_or_default();
        channel_clone.channel_tag = channel_tag.clone().unwrap_or_default();
        channel_clone.parent_channel_tag = parent_channel_tag.clone();

        let result = channel_insert(
            &self.rb,
            device_tag.as_deref(),
            channel_tag.as_deref(),
            Some(&channel.name),
            Some(&status_str),
            Some(&device_type_str),
            channel.device_type_code.as_deref(),
            channel_extended_json,
            Some(channel.is_default),
            parent_channel_tag.as_deref(),
            channel.civil_code.as_deref(),
            channel.address.as_deref(),
            channel.ip_address.as_deref(),
            Some(channel.port as i32),
            channel.manufacturer.as_deref(),
            channel.model.as_deref(),
            channel.latitude,
            channel.longitude,
            Some(channel.parental),
            Some(extended_json),
            Some(RbatisDateTime::from_timestamp_millis(channel.created_at.timestamp_millis())),
            channel.updated_at.map(|u| RbatisDateTime::from_timestamp_millis(u.timestamp_millis())),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        let id = result.last_insert_id.as_i64().unwrap_or(0);
        let key = format_cache_key_opt(device_tag.as_deref(), channel_tag.as_deref());
        let cache = self.channels_cache.clone();
        tokio::spawn(async move {
            cache.insert(key, channel_clone);
        });
        Ok(id)
    }

    pub async fn update_channel(&self, channel: &Channel) -> Result<()> {
        let device_tag = if channel.device_tag.is_empty() { String::new() } else { channel.device_tag.to_lowercase() };
        let channel_tag = if channel.channel_tag.is_empty() { String::new() } else { channel.channel_tag.to_lowercase() };
        let parent_channel_tag = channel.parent_channel_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let status_str = channel.status.to_string();
        let device_type_str = channel.device_type.to_string();
        let channel_extended_json = channel.channel_extended.clone();
        let extended_json = channel.extended.clone().unwrap_or(serde_json::Value::Object(Default::default()));
        let mut channel_clone = channel.clone();
        channel_clone.device_tag = device_tag.clone();
        channel_clone.channel_tag = channel_tag.clone();
        channel_clone.parent_channel_tag = parent_channel_tag.clone();

        channel_update(
            &self.rb,
            &device_tag,
            &channel_tag,
            Some(&channel.name),
            Some(&status_str),
            Some(&device_type_str),
            channel.device_type_code.as_deref(),
            channel_extended_json,
            Some(channel.is_default),
            parent_channel_tag.as_deref(),
            channel.civil_code.as_deref(),
            channel.address.as_deref(),
            channel.ip_address.as_deref(),
            Some(channel.port as i32),
            channel.manufacturer.as_deref(),
            channel.model.as_deref(),
            Some(channel.parental),
            Some(extended_json),
            Some(RbatisDateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis())),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        let key = format_cache_key(&device_tag, &channel_tag);
        let cache = self.channels_cache.clone();
        tokio::spawn(async move {
            cache.insert(key, channel_clone);
        });

        Ok(())
    }

    pub async fn get_channels_by_device_tag(&self, device_tag: &str) -> Result<Vec<Channel>> {
        let rows = channel_select_by_device_tag(&self.rb, device_tag)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Channel::try_from(r).ok()).collect())
    }

    pub async fn get_channel(&self, device_tag: &str, channel_tag: &str) -> Result<Option<Channel>> {
        let device_tag = device_tag.to_lowercase();
        let channel_tag = channel_tag.to_lowercase();
        let rows = channel_select_by_device_tag_and_channel_tag(
            &self.rb,
            &device_tag,
            &channel_tag,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Channel::try_from(r).ok()).next())
    }

    pub fn get_channels_by_device_tag_cached(&self, device_tag: &str) -> Vec<Channel> {
        self.channels_cache.iter()
            .filter(|c| c.device_tag == device_tag)
            .map(|r| r.clone())
            .collect()
    }

    pub fn get_audio_broadcast_channel_cached(&self, device_tag: &str) -> Option<Channel> {
        self.channels_cache.iter()
            .filter(|c| c.device_tag == device_tag && c.is_audio_channel())
            .map(|r| r.clone())
            .next()
    }

    pub fn list_all_channels_cached(&self) -> Vec<Channel> {
        let mut channels: Vec<Channel> = self.channels_cache.iter().map(|r| r.clone()).collect();
        channels.sort_by(|a, b| b.id.cmp(&a.id));
        channels
    }

    pub async fn list_all_channels(&self) -> Result<Vec<Channel>> {
        let rows = channel_select_all(&self.rb)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Channel::try_from(r).ok()).collect())
    }

    pub async fn list_channels_paginated(
        &self,
        device_tag: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Channel>> {
        let rows = channel_select_paginated(
            &self.rb,
            device_tag.map(String::from),
            search.map(String::from),
            status.map(String::from),
            limit as i64,
            offset as i64,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Channel::try_from(r).ok()).collect())
    }

    pub async fn count_channels(
        &self,
        device_tag: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
    ) -> Result<usize> {
        let count = channel_count(
            &self.rb,
            device_tag.map(String::from),
            search.map(String::from),
            status.map(String::from),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(count as usize)
    }

    pub async fn delete_channel(&self, device_tag: &str, channel_tag: &str) -> Result<()> {
        channel_delete_by_device_tag_and_channel_tag(&self.rb, device_tag, channel_tag)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        let key = format_cache_key(device_tag, channel_tag);
        self.channels_cache.remove(&key);
        Ok(())
    }

    pub async fn list_groups(&self) -> Vec<DeviceGroup> {
        self.groups_cache.iter().map(|g| g.value().clone()).collect()
    }

    pub async fn create_group(&self, req: &CreateGroupRequest) -> Result<i64> {
        let result = group_insert(
            &self.rb,
            &req.name,
            req.parent_id,
            req.sort_order.unwrap_or(0) as i32,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(result.last_insert_id.as_i64().unwrap_or(0))
    }

    pub async fn update_group(&self, id: i64, req: &UpdateGroupRequest) -> Result<()> {
        group_update(
            &self.rb,
            id,
            req.name.as_deref().unwrap_or(""),
            req.parent_id,
            req.sort_order.unwrap_or(0) as i32,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn delete_group(&self, id: i64) -> Result<()> {
        group_delete(&self.rb, id)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn get_group_tree(&self) -> Vec<DeviceGroupNode> {
        let groups: Vec<DeviceGroupNode> = self.groups_cache.iter()
            .map(|g| DeviceGroupNode {
                id: g.value().id,
                name: g.value().name.clone(),
                parent_id: g.value().parent_id,
                sort_order: g.value().sort_order,
                own_device_count: g.value().device_count,
                device_count: g.value().device_count,
                children: Vec::new(),
            })
            .collect();
        build_group_tree_from_nodes(groups)
    }

    pub async fn list_regions(&self) -> Vec<Region> {
        self.regions_cache.iter().map(|r| r.value().clone()).collect()
    }

    pub async fn list_region_children(&self, parent_code: &str) -> Vec<Region> {
        self.regions_cache.iter()
            .filter(|r| r.value().parent_code.as_deref() == Some(parent_code))
            .map(|r| r.value().clone())
            .collect()
    }

    pub async fn get_region_tree(&self) -> Vec<RegionNode> {
        let nodes: Vec<RegionNode> = self.regions_cache.iter()
            .map(|r| RegionNode {
                code: r.value().code.clone(),
                name: r.value().name.clone(),
                level: r.value().level,
                gb28181_code: r.value().gb28181_code.clone(),
                parent_code: r.value().parent_code.clone(),
                own_device_count: r.value().device_count,
                device_count: r.value().device_count,
                children: Vec::new(),
            })
            .collect();
        build_region_tree_from_nodes(nodes)
    }

    pub fn get_region_descendant_codes(&self, region_code: &str) -> Vec<String> {
        let nodes: Vec<RegionNode> = self.regions_cache.iter()
            .map(|r| RegionNode {
                code: r.value().code.clone(),
                name: r.value().name.clone(),
                level: r.value().level,
                gb28181_code: r.value().gb28181_code.clone(),
                parent_code: r.value().parent_code.clone(),
                own_device_count: r.value().device_count,
                device_count: r.value().device_count,
                children: Vec::new(),
            })
            .collect();
        let tree = build_region_tree_from_nodes(nodes);

        fn find_and_collect(node: &RegionNode, target: &str) -> Vec<String> {
            if node.code == target {
                return collect_descendant_codes(node);
            }
            for child in &node.children {
                let result = find_and_collect(child, target);
                if !result.is_empty() {
                    return result;
                }
            }
            Vec::new()
        }

        for root in &tree {
            let codes = find_and_collect(root, region_code);
            if !codes.is_empty() {
                return codes;
            }
        }
        vec![region_code.to_string()]
    }

    pub fn servers_cache(&self) -> &Arc<DashMap<i64, ServerRow>> {
        &self.servers_cache
    }

    pub async fn create_server(&self, server: &Server) -> Result<i64> {
        let protocol_ports_json = serde_json::to_value(&server.protocol_ports).unwrap_or(serde_json::Value::Object(Default::default()));
        server_insert(
            &self.rb,
            Some(&server.name),
            Some(&server.url),
            Some(&server.api_key),
            Some(&server.server_type.to_string()),
            Some(server.weight as i32),
            Some(server.enabled),
            Some(&server.server_tag),
            Some(protocol_ports_json),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        let rows = server_select_by_tag(&self.rb, &server.server_tag)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let row = rows.into_iter().next()
            .ok_or_else(|| AppError::Internal("Failed to get inserted server".to_string()))?;

        self.servers_cache.insert(row.id, row.clone());
        Ok(row.id)
    }

    pub async fn update_server(&self, server: &Server) -> Result<()> {
        let protocol_ports_json = serde_json::to_value(&server.protocol_ports).unwrap_or(serde_json::Value::Object(Default::default()));
        server_update(
            &self.rb,
            server.id,
            &server.name,
            &server.url,
            &server.api_key,
            &server.server_type.to_string(),
            server.weight as i32,
            server.enabled,
            &server.server_tag,
            protocol_ports_json,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        self.servers_cache.insert(server.id, ServerRow {
            id: server.id,
            name: server.name.clone(),
            url: server.url.clone(),
            api_key: server.api_key.clone(),
            server_type: server.server_type.to_string(),
            weight: server.weight as i32,
            enabled: server.enabled,
            server_tag: server.server_tag.clone(),
            protocol_ports: serde_json::to_value(&server.protocol_ports).unwrap_or_default(),
            created_at: RbatisDateTime::from_timestamp_millis(server.created_at.timestamp_millis()),
            updated_at: RbatisDateTime::from_timestamp_millis(server.updated_at.timestamp_millis()),
        });
        Ok(())
    }

    pub async fn delete_server(&self, id: i64) -> Result<()> {
        server_delete_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        self.servers_cache.remove(&id);
        Ok(())
    }

    pub async fn server_exists_by_tag(&self, tag: &str) -> anyhow::Result<bool> {
        match server_select_by_tag(&self.rb, tag).await {
            Ok(rows) => Ok(!rows.is_empty()),
            Err(e) => {
                tracing::warn!("[DbRepository] server_exists_by_tag failed: {}", e);
                Ok(false)
            }
        }
    }

    pub async fn get_server_by_tag(&self, tag: &str) -> Result<Option<crate::domain::server::Server>> {
        let rows = server_select_by_tag(&self.rb, tag)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|r| crate::domain::server::Server::try_from(r).ok()))
    }

    pub fn sessions_cache(&self) -> &Arc<DashMap<i64, Session>> {
        &self.sessions_cache
    }

    pub async fn create_session(&self, session: &Session) -> Result<i64> {
        let session_type_str = session.session_type.to_string();
        let state_str = session.state.to_string();
        let protocol_str = session.protocol.as_ref().map(|p| p.to_string());
        let expires_rb = session.expires_at.map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis()));
        let created_at_rb = RbatisDateTime::from_timestamp_millis(session.created_at.timestamp_millis());
        let last_activity_rb = RbatisDateTime::from_timestamp_millis(session.last_activity.timestamp_millis());
        let id = session_insert(
            &self.rb,
            Some(&session_type_str),
            session.device_tag.as_deref(),
            session.channel_tag.as_deref(),
            Some(session.user_id),
            Some(&state_str),
            session.client_ip.as_deref(),
            session.client_type.as_deref(),
            session.tcp_id.as_deref(),
            session.media_server_tag.as_deref(),
            protocol_str.as_deref(),
            Some(created_at_rb),
            Some(last_activity_rb),
            expires_rb,
            Some(session.bytes_sent as i64),
            Some(session.bytes_received as i64),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        id.ok_or_else(|| AppError::Internal("session insert returned no id".to_string()))
    }

    pub async fn list_sessions_paginated(&self, search: Option<&str>, state: Option<&str>, limit: usize, offset: usize) -> Vec<Session> {
        let rows = match session_select_paginated(
            &self.rb,
            search.map(String::from),
            state.map(String::from),
            limit as i64,
            offset as i64,
        ).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("list_sessions_paginated: {}", e);
                return Vec::new();
            }
        };

        rows.into_iter().filter_map(|row| {
            Session::try_from(row).ok()
        }).collect()
    }

    pub async fn count_sessions(&self, search: Option<&str>, state: Option<&str>) -> usize {
        match session_count(
            self.rb.rb_ref(),
            search.map(String::from),
            state.map(String::from),
        ).await {
            Ok(count) => count as usize,
            Err(e) => {
                tracing::error!("count_sessions: {}", e);
                self.sessions_cache.len()
            }
        }
    }

    pub async fn delete_session(&self, id: i64) -> Result<()> {
        session_delete_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        self.sessions_cache.remove(&id);
        Ok(())
    }

    pub fn streams_cache(&self) -> &Arc<DashMap<String, Stream>> {
        &self.streams_cache
    }

    pub async fn create_stream(&self, stream: &Stream) -> Result<()> {
        let device_tag = stream.device_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let channel_tag = stream.channel_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let id = stream_insert(
            &self.rb,
            device_tag.as_deref(),
            channel_tag.as_deref(),
            Some(stream.media_server_tag.clone()),
            &stream.app,
            Some(&stream.token),
            &stream.state.to_string(),
            Some(stream.retry_count as i32),
            Some(stream.max_retries as i32),
            stream.last_error.clone(),
            Some(stream.viewer_count as i32),
            Some(stream.bandwidth_in as i64),
            Some(stream.bandwidth_out as i64),
            RbatisDateTime::from_timestamp_millis(stream.last_keepalive_at.timestamp_millis()),
            RbatisDateTime::from_timestamp_millis(stream.created_at.timestamp_millis()),
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        let id = id.ok_or_else(|| AppError::Internal("stream insert returned no id".to_string()))?;
        let mut stream = stream.clone();
        stream.id = id;
        stream.device_tag = device_tag;
        stream.channel_tag = channel_tag;

        if let (Some(dt), Some(ct)) = (
            stream.device_tag.as_deref(),
            stream.channel_tag.as_deref(),
        ) {
            if let Err(e) = stream_delete_others(&self.rb, dt, ct, id).await {
                tracing::error!("delete duplicate streams of {}/{}: {}", dt, ct, e);
            }
        }

        let cache_key = format_cache_key_opt(stream.device_tag.as_deref(), stream.channel_tag.as_deref());
        self.streams_cache.insert(cache_key, stream);
        Ok(())
    }

    pub async fn update_session_state(&self, session: &Session) -> Result<()> {
        session_update_state(
            &self.rb,
            &session.state.to_string(),
            RbatisDateTime::from_timestamp_millis(session.last_activity.timestamp_millis()),
            session.id,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn update_session_states_by_stream(
        &self,
        device_tag: &str,
        channel_tag: &str,
        state: crate::domain::SessionState,
    ) -> Result<()> {
        session_update_states_by_stream(
            &self.rb,
            &state.to_string(),
            RbatisDateTime::from_timestamp_millis(chrono::Utc::now().timestamp_millis()),
            device_tag,
            channel_tag,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn delete_streams_by_device(&self, device_tag: &str) -> Result<()> {
        let cache_key_prefix = format!("{}/", device_tag);
        self.streams_cache.retain(|k, _| !k.starts_with(&cache_key_prefix));
        Ok(())
    }

    pub fn get_stream_cache_key_by_stream_key(&self, stream_key: &str) -> Option<String> {
        let cache_key = stream_key_to_cache_key(stream_key);
        if self.streams_cache.contains_key(&cache_key) {
            Some(cache_key)
        } else {
            None
        }
    }

    pub async fn update_stream(&self, stream: &Stream) -> Result<()> {
        let device_tag = stream.device_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let channel_tag = stream.channel_tag.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        let cache_key = format_cache_key_opt(device_tag.as_deref(), channel_tag.as_deref());

        stream_update(
            &self.rb,
            device_tag.as_deref(),
            channel_tag.as_deref(),
            Some(stream.media_server_tag.clone()),
            Some(&stream.state.to_string()),
            Some(stream.retry_count as i32),
            Some(stream.max_retries as i32),
            stream.last_error.clone(),
            Some(stream.viewer_count as i32),
            Some(stream.bandwidth_in as i64),
            Some(stream.bandwidth_out as i64),
            Some(RbatisDateTime::from_timestamp_millis(stream.last_keepalive_at.timestamp_millis())),
            stream.id,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        let mut stream = stream.clone();
        stream.device_tag = device_tag;
        stream.channel_tag = channel_tag;
        self.streams_cache.insert(cache_key, stream);
        Ok(())
    }

    pub async fn get_stream_by_token(&self, token: &str) -> Result<Option<Stream>> {
        let rows = stream_select_by_token(
            &self.rb,
            token,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        match rows.first() {
            Some(row) => Ok(Some(Stream::try_from(row.clone())?)),
            None => Ok(None),
        }
    }

    pub async fn list_streams(&self) -> Vec<Stream> {
        self.streams_cache.iter().map(|r| r.clone()).collect()
    }

    pub async fn list_streams_paginated(&self, search: Option<&str>, state: Option<&str>, limit: usize, offset: usize) -> Vec<Stream> {
        let rows = match stream_select_paginated(
            &self.rb,
            search.map(String::from),
            state.map(String::from),
            limit as i64,
            offset as i64,
        ).await {
            Ok(r) => {
                r
            }
            Err(e) => {
                tracing::error!("list_streams_paginated: {}", e);
                return Vec::new();
            }
        };

        let cache = self.streams_cache.clone();
        rows.into_iter().filter_map(|row| {
            let mut stream = Stream::try_from(row).ok()?;
            let cache_key = format_cache_key_opt(stream.device_tag.as_deref(), stream.channel_tag.as_deref());
            if let Some(cached) = cache.get(&cache_key) {
                stream.state = cached.state;
                stream.last_keepalive_at = cached.last_keepalive_at;
                stream.last_error = cached.last_error.clone();
                stream.viewer_count = cached.viewer_count;
                stream.bandwidth_in = cached.bandwidth_in;
                stream.bandwidth_out = cached.bandwidth_out;
            }
            Some(stream)
        }).collect()
    }

    pub async fn count_streams(&self, search: Option<&str>, state: Option<&str>) -> usize {
        match stream_count(
            self.rb.rb_ref(),
            search.map(String::from),
            state.map(String::from),
        ).await {
            Ok(count) => count as usize,
            Err(e) => {
                tracing::error!("count_streams: {}", e);
                self.streams_cache.len()
            }
        }
    }

    pub async fn create_recording(&self, recording: &Recording) -> Result<i64> {
        let result = recording_insert(
            &self.rb,
            recording.device_tag.as_deref(),
            recording.channel_tag.as_deref(),
            &recording.server_tag,
            &recording.state.to_string(),
            &recording.format.to_string(),
            recording.output_path.clone(),
            recording.file_size as i64,
            recording.duration_secs as i64,
            RbatisDateTime::from_timestamp_millis(recording.created_at.timestamp_millis()),
            recording.started_at.map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording.stopped_at.map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording.error_message.clone(),
            serde_json::to_string(&recording.labels).ok(),
            recording.filename.clone(),
            recording.max_duration_secs.map(|v| v as i32),
            &recording.recording_type.to_string(),
            recording.alarm_id,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;

        result.into_iter().next().map(|r| r.id)
            .ok_or_else(|| AppError::Internal("Failed to insert recording".to_string()))
    }

    pub async fn update_recording(&self, recording: &Recording) -> Result<()> {
        recording_update(
            &self.rb,
            recording.device_tag.as_deref(),
            recording.channel_tag.as_deref(),
            Some(recording.server_tag.clone()),
            Some(recording.state.to_string()),
            Some(recording.format.to_string()),
            recording.output_path.clone(),
            Some(recording.file_size as i64),
            Some(recording.duration_secs as i64),
            recording.started_at.map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording.stopped_at.map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording.error_message.clone(),
            serde_json::to_string(&recording.labels).ok(),
            recording.filename.clone(),
            Some(recording.recording_type.to_string()),
            recording.id,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn update_recording_start_result(&self, recording: &Recording) -> Result<()> {
        recording_update_start_result(
            &self.rb,
            &recording.state.to_string(),
            recording.error_message.as_deref(),
            recording.retry_count,
            recording
                .next_retry_at
                .map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording
                .started_at
                .map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording
                .stopped_at
                .map(|dt| RbatisDateTime::from_timestamp_millis(dt.timestamp_millis())),
            recording.output_path.as_deref(),
            recording.file_size as i64,
            recording.duration_secs as i64,
            recording.id,
        ).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn list_pending_recordings(&self) -> Result<Vec<Recording>> {
        let rows = recording_select_pending(&self.rb)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Recording::try_from(r).ok()).collect())
    }

    pub async fn find_pending_recording_by_stream(
        &self,
        device_tag: &str,
        channel_tag: &str,
        recording_type: &str,
    ) -> Result<Option<Recording>> {
        let rows = recording_select_pending_by_stream(&self.rb, device_tag, channel_tag, recording_type)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().find_map(|r| Recording::try_from(r).ok()))
    }

    pub async fn list_recordings_paginated(&self, limit: usize, offset: usize) -> Result<Vec<Recording>> {
        self.list_recordings_filtered(None, None, None, None, limit, offset).await
    }

    pub async fn list_recordings_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        state: Option<&str>,
        search: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Recording>> {
        let rows = recording_select_paginated(
            &self.rb,
            device_tag,
            channel_tag,
            state,
            search,
            limit as i64,
            offset as i64,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        let recordings: Vec<Recording> = rows.into_iter().filter_map(|r| Recording::try_from(r).ok()).collect();
        Ok(recordings)
    }

    pub async fn count_recordings(&self) -> Result<usize> {
        self.count_recordings_filtered(None, None, None, None).await
    }

    pub async fn count_recordings_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        state: Option<&str>,
        search: Option<&str>,
    ) -> Result<usize> {
        let count = recording_count(&self.rb, device_tag, channel_tag, state, search)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(count as usize)
    }

  pub async fn find_active_recording_by_stream_key(&self, stream_key: &str) -> Result<Option<Recording>> {
      Ok(self.find_active_recordings_by_stream_key(stream_key).await?.into_iter().next())
  }

  /// 同一个流上可能有多个并行的录像任务(定时/报警)，它们共享媒体服务器上的同一份录制。
  pub async fn find_active_recordings_by_stream_key(&self, stream_key: &str) -> Result<Vec<Recording>> {
      let Some((device_tag, channel_tag)) = crate::common::device_utils::parse_stream_key_loose(stream_key) else {
          return Ok(Vec::new());
      };
      let rows = recording_select_active_by_stream(&self.rb, &device_tag, &channel_tag)
          .await.map_err(|e| AppError::Internal(e.to_string()))?;
      Ok(rows.into_iter().filter_map(|r| Recording::try_from(r).ok()).collect())
  }

  pub async fn list_active_recordings(&self) -> Result<Vec<Recording>> {
      let rows = recording_select_active(&self.rb)
          .await.map_err(|e| AppError::Internal(e.to_string()))?;
      Ok(rows.into_iter().filter_map(|r| Recording::try_from(r).ok()).collect())
  }


    pub async fn get_recording(&self, id: i64) -> Result<Option<Recording>> {
        let rows = recording_select_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| Recording::try_from(r).ok()).next())
    }

    pub async fn delete_recording(&self, id: i64) -> Result<()> {
        // 先删录像分段：recording_segments 外键指向 recordings，不先删会违反外键约束
        recording_delete_segments_by_recording_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        recording_delete_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn list_recordings_by_device(&self, device_tag: &str) -> Result<Vec<Recording>> {
        self.list_recordings_by_device_tag(device_tag).await
    }

    pub async fn list_recordings_by_device_tag(&self, device_tag: &str) -> Result<Vec<Recording>> {
        let rows = recording_select_by_device_tag(&self.rb, device_tag)
            .await.map_err(|e| AppError::Internal(e.to_string()))?;
        let recordings: Vec<Recording> = rows.into_iter()
            .filter_map(|r| Recording::try_from(r).ok())
            .collect();
        Ok(recordings)
    }

    pub async fn log_ptz_control(
        &self,
        user_id: Option<String>,
        device_id: i64,
        command: &str,
        speed: f64,
        result: bool,
        call_id: Option<String>,
        error_message: Option<String>,
    ) -> Result<()> {
        let _ = (user_id, device_id, command, speed, result, call_id, error_message);
        Ok(())
    }

    pub async fn log_ptz_result(
        &self,
        device_id: i64,
        call_id: Option<&str>,
        sip_code: Option<u16>,
        status: &str,
        message: Option<String>,
    ) -> Result<()> {
        let _ = (device_id, call_id, sip_code, status, message);
        Ok(())
    }

    pub async fn list_ptz_presets(&self, _device_id: i64) -> Result<Vec<PtzPreset>> {
        Ok(Vec::new())
    }

    pub async fn create_ptz_preset(&self, device_id: i64, name: &str) -> Result<PtzPreset> {
        let token = uuid::Uuid::new_v4().to_string();
        let preset = PtzPreset::new(device_id, name.to_string(), token);
        Ok(preset)
    }

    pub async fn create_ptz_preset_with_token(
        &self,
        device_id: i64,
        name: &str,
        token: &str,
    ) -> Result<PtzPreset> {
        let preset = PtzPreset::new(device_id, name.to_string(), token.to_string());
        Ok(preset)
    }

    pub async fn update_ptz_preset(&self, device_id: i64, token: &str, name: &str) -> Result<()> {
        let device_uuid = {
            let rows: Vec<crate::sql_mappers::DeviceRow> = crate::sql_mappers::device_select_by_id(&self.rb, device_id).await?;
            rows.into_iter().next()
                .map(|r| r.id.to_string())
                .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?
        };
        crate::sql_mappers::ptz_preset_update(&self.rb, device_uuid, name.to_string(), token.to_string()).await?;
        Ok(())
    }

    pub async fn delete_ptz_preset(&self, _device_id: i64, _token: &str) -> Result<()> {
        Ok(())
    }

    pub async fn get_layout(&self, id: i64) -> Option<PlayerLayout> {
        layout_select_by_id(&self.rb, id).await.ok()
            .and_then(|rows| rows.into_iter().next().map(PlayerLayout::from))
    }

    pub async fn list_layouts(&self) -> Vec<PlayerLayout> {
        layout_select_all(&self.rb).await.unwrap_or_default()
            .into_iter().map(PlayerLayout::from).collect()
    }

    pub async fn get_default_layout(&self) -> Option<PlayerLayout> {
        layout_select_default(&self.rb).await.ok()
            .and_then(|rows| rows.into_iter().next().map(PlayerLayout::from))
    }

    pub async fn set_default_layout(&self, id: i64) -> Result<()> {
        layout_set_default(&self.rb).await.map_err(|e| AppError::Internal(e.to_string()))?;
        layout_update(&self.rb, id, None::<&str>, None, None, None, Some(true)).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn create_layout(&self, name: &str, rows: i32, cols: i32, layout_json: serde_json::Value, is_default: bool) -> Result<i64> {
        if is_default {
            let _ = layout_set_default(&self.rb).await;
        }
        let rows = layout_insert(&self.rb, name, rows, cols, layout_json, is_default).await?;
        rows.into_iter().next().map(|r| r.id).ok_or_else(|| AppError::Internal("Failed to insert layout".to_string()))
    }

    pub async fn update_layout(
        &self,
        id: i64,
        name: Option<String>,
        rows: Option<i32>,
        cols: Option<i32>,
        layout_json: Option<serde_json::Value>,
        is_default: Option<bool>,
    ) -> Result<()> {
        if is_default == Some(true) {
            let _ = layout_set_default(&self.rb).await;
        }
        let name_ref = name.as_deref();
        layout_update(&self.rb, id, name_ref, rows, cols, layout_json, is_default).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn delete_layout(&self, id: i64) -> Result<()> {
        layout_delete(&self.rb, id).await.map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn get_map_settings(&self) -> Option<MapSettings> {
        map_settings_select(&self.rb).await.ok()
            .and_then(|rows| rows.into_iter().next().map(MapSettings::from))
    }

    pub async fn upsert_map_settings(
        &self,
        browser_tms_url: &str,
        client_tms_url: &str,
        wfs_url: &str,
        browser_min_zoom: i32,
        browser_max_zoom: i32,
        client_min_zoom: i32,
        client_max_zoom: i32,
    ) -> Result<()> {
        map_settings_upsert(
            &self.rb,
            browser_tms_url,
            client_tms_url,
            wfs_url,
            browser_min_zoom,
            browser_max_zoom,
            client_min_zoom,
            client_max_zoom,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

fn protocol_to_str(p: &Protocol) -> &'static str {
    match p {
        Protocol::Gb28181 => "GB28181",
        Protocol::Onvif => "ONVIF",
        Protocol::Rtsp => "RTSP",
        Protocol::Rtmp => "RTMP",
        Protocol::Hls => "HLS",
        Protocol::WebRTC => "WebRTC",
        Protocol::Custom(name) => Box::leak(name.clone().into_boxed_str()),
    }
}

fn status_to_str(s: &DeviceStatus) -> &'static str {
    match s {
        DeviceStatus::Offline => "offline",
        DeviceStatus::Online => "online",
        DeviceStatus::Error => "error",
        DeviceStatus::Maintaining => "maintaining",
    }
}

fn build_group_tree(rows: Vec<GroupRow>, group_stats: &std::collections::HashMap<String, u32>) -> Vec<DeviceGroupNode> {
    let nodes: Vec<DeviceGroupNode> = rows.iter()
        .filter_map(|r| {
            let mut node = DeviceGroupNode::try_from(r.clone()).ok()?;
            node.device_count = group_stats.get(&node.id.to_string()).copied().unwrap_or(0);
            Some(node)
        })
        .collect();

    build_group_tree_from_nodes(nodes)
}

fn build_group_tree_from_nodes(nodes: Vec<DeviceGroupNode>) -> Vec<DeviceGroupNode> {
    fn build_children(nodes: &[DeviceGroupNode], parent_id: Option<i64>, visited: &mut std::collections::HashSet<i64>) -> Vec<DeviceGroupNode> {
        let mut result = Vec::new();
        for n in nodes.iter() {
            if n.parent_id == parent_id && !visited.contains(&n.id) {
                visited.insert(n.id);
                let mut node = n.clone();
                node.children = build_children(nodes, Some(n.id), visited);
                node.device_count = calculate_group_recursive_count(&node);
                visited.remove(&n.id);
                result.push(node);
            }
        }
        result
    }

    let roots = build_children(&nodes, None, &mut Default::default());
    roots
}

fn build_region_tree_from_nodes(nodes: Vec<RegionNode>) -> Vec<RegionNode> {
    fn build_children(nodes: &[RegionNode], parent_code: &str, visited: &mut std::collections::HashSet<String>) -> Vec<RegionNode> {
        let mut result: Vec<RegionNode> = nodes.iter()
            .filter(|n| n.parent_code.as_deref() == Some(parent_code) && !visited.contains(&n.code))
            .cloned()
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));

        for n in result.iter_mut() {
            visited.insert(n.code.clone());
            n.children = build_children(nodes, &n.code, visited);
            n.device_count = calculate_recursive_count(n);
            visited.remove(&n.code);
        }
        result
    }

    let mut roots: Vec<RegionNode> = nodes.iter()
        .filter(|n| n.parent_code.is_none())
        .cloned()
        .collect();
    roots.sort_by(|a, b| a.name.cmp(&b.name));

    for n in roots.iter_mut() {
        n.children = build_children(&nodes, &n.code, &mut Default::default());
        n.device_count = calculate_recursive_count(n);
    }
    roots
}

fn calculate_recursive_count(node: &RegionNode) -> u32 {
    let children_sum: u32 = node.children.iter().map(|c| calculate_recursive_count(c)).sum();
    node.own_device_count + children_sum
}

fn calculate_group_recursive_count(node: &DeviceGroupNode) -> u32 {
    let children_sum: u32 = node.children.iter().map(|c| calculate_group_recursive_count(c)).sum();
    node.own_device_count + children_sum
}

fn collect_descendant_codes(node: &RegionNode) -> Vec<String> {
    let mut codes = vec![node.code.clone()];
    for child in &node.children {
        codes.extend(collect_descendant_codes(child));
    }
    codes
}

impl TryFrom<SessionRow> for Session {
    type Error = AppError;

    fn try_from(row: SessionRow) -> Result<Self> {
        use crate::domain::{SessionState, SessionType};

        let session_type = match row.session_type.as_str() {
            "play" => SessionType::Play,
            "record" => SessionType::Record,
            "playback" => SessionType::Playback,
            "ptz" => SessionType::Ptz,
            other => return Err(AppError::Internal(format!("unknown session_type: {other}"))),
        };

        let state = match row.state.as_str() {
            "initializing" => SessionState::Initializing,
            "active" => SessionState::Active,
            "idle" => SessionState::Idle,
            "terminating" => SessionState::Terminating,
            "terminated" => SessionState::Terminated,
            other => return Err(AppError::Internal(format!("unknown session state: {other}"))),
        };

        let user_id = row.user_id;

        let protocol = match row.protocol.as_ref() {
            Some(p) => Some(match p.as_str() {
                "GB28181" => Protocol::Gb28181,
                "ONVIF" => Protocol::Onvif,
                "RTSP" => Protocol::Rtsp,
                "RTMP" => Protocol::Rtmp,
                "HLS" => Protocol::Hls,
                "WebRTC" => Protocol::WebRTC,
                other => return Err(AppError::Internal(format!("unknown protocol: {other}"))),
            }),
            None => None,
        };

        let created_at = chrono::DateTime::from_timestamp(row.created_at.unix_timestamp(), 0)
            .unwrap_or_else(chrono::Utc::now);

        let last_activity = chrono::DateTime::from_timestamp(row.last_activity.unix_timestamp(), 0)
            .unwrap_or_else(chrono::Utc::now);

        let expires_at = row.expires_at.and_then(|s| chrono::DateTime::from_timestamp(s.unix_timestamp(), 0));

        Ok(Session {
            id: row.id,
            session_type,
            device_tag: row.device_tag,
            channel_tag: row.channel_tag,
            user_id,
            state,
            client_ip: row.client_ip,
            client_type: row.client_type,
            tcp_id: row.tcp_id,
            media_server_tag: row.media_server_tag,
            protocol,
            created_at,
            last_activity,
            expires_at,
            bytes_sent: row.bytes_sent as u64,
            bytes_received: row.bytes_received as u64,
        })
    }
}

impl DbRepository {
    pub async fn save_alarm(&self, alarm: &CreateAlarmRequest) -> Result<Alarm> {
        let alarm_time = RbatisDateTime::from_timestamp_millis(alarm.alarm_time.timestamp_millis());
        let rows = alarm_insert(
            &self.rb,
            alarm.device_id,
            &alarm.device_tag,
            alarm.channel_tag.as_deref(),
            &alarm.alarm_type,
            alarm_time,
            alarm.alarm_method.unwrap_or(1),
            alarm.alarm_priority.unwrap_or(0),
            alarm.description.as_deref(),
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        rows.into_iter().next().map(|r| r.into()).ok_or_else(|| AppError::Internal("Failed to insert alarm".to_string()))
    }

    pub async fn list_alarms(&self, device_id: Option<i64>, limit: usize, offset: usize) -> Result<Vec<Alarm>> {
        let limit = limit as i64;
        let offset = offset as i64;
        let rows = if let Some(did) = device_id {
            alarm_select_by_device(&self.rb, did, limit, offset)
                .await
                .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        } else {
            alarm_select_all(&self.rb, limit, offset)
                .await
                .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        };
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn list_alarms_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        alarm_type: Option<&str>,
        start_time: Option<chrono::DateTime<Utc>>,
        end_time: Option<chrono::DateTime<Utc>>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Alarm>> {
        let start_rb = start_time.map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));
        let end_rb = end_time.map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));
        let rows = alarm_select_filtered(
            &self.rb,
            device_tag,
            channel_tag,
            alarm_type,
            start_rb,
            end_rb,
            limit as i64,
            offset as i64,
        )
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn count_alarms_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        alarm_type: Option<&str>,
        start_time: Option<chrono::DateTime<Utc>>,
        end_time: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64> {
        let start_rb = start_time.map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));
        let end_rb = end_time.map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));
        let count = alarm_count_filtered(&self.rb, device_tag, channel_tag, alarm_type, start_rb, end_rb)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(count)
    }

    pub async fn count_alarms(&self, device_id: Option<i64>) -> Result<i64> {
        let count = if let Some(did) = device_id {
            alarm_count_by_device(&self.rb, did)
                .await
                .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        } else {
            alarm_count_all(&self.rb)
                .await
                .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        };
        Ok(count)
    }

    pub async fn mark_alarm_processed(&self, alarm_id: i64, processed: bool) -> Result<()> {
        alarm_mark_processed(&self.rb, processed, alarm_id)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(())
    }

    pub async fn get_alarm(&self, alarm_id: i64) -> Result<Option<crate::domain::alarm::Alarm>> {
        let rows = alarm_select_by_id(&self.rb, alarm_id)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().next().map(|r| crate::domain::alarm::Alarm::from(r)))
    }

    pub async fn create_recording_policy(&self, policy: &crate::domain::recording_policy::RecordingPolicy) -> Result<i64> {
        let created_at = RbatisDateTime::from_timestamp_millis(policy.created_at.timestamp_millis());
        let updated_at = RbatisDateTime::from_timestamp_millis(policy.updated_at.timestamp_millis());

        let result = policy_insert(
            &self.rb,
            &policy.device_tag,
            &policy.channel_tag,
            &policy.policy_type.to_string(),
            policy.enabled,
            policy.schedule_type.map(|s| s.to_string()).as_deref(),
            policy.cron_expr.as_deref(),
            policy.alarm_type.as_deref(),
            policy.pre_record_secs as i32,
            policy.post_record_secs as i32,
            &policy.format.to_string(),
            policy.max_duration_secs as i32,
            policy.max_file_size_mb as i32,
            policy.output_path.as_deref(),
            Some(created_at),
            Some(updated_at),
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;

        result.into_iter().next().map(|r| r.id)
            .ok_or_else(|| AppError::Internal("Failed to insert recording policy".to_string()))
    }

    pub async fn update_recording_policy(&self, policy: &crate::domain::recording_policy::RecordingPolicy) -> Result<()> {
        policy_update(
            &self.rb,
            policy.id,
            Some(&policy.device_tag),
            Some(&policy.channel_tag),
            Some(&policy.policy_type.to_string()),
            Some(policy.enabled),
            policy.schedule_type.map(|s| s.to_string()).as_deref(),
            policy.cron_expr.as_deref(),
            policy.alarm_type.as_deref(),
            Some(policy.pre_record_secs as i32),
            Some(policy.post_record_secs as i32),
            Some(&policy.format.to_string()),
            Some(policy.max_duration_secs as i32),
            Some(policy.max_file_size_mb as i32),
            policy.output_path.as_deref(),
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(())
    }

    pub async fn delete_recording_policy(&self, id: i64) -> Result<()> {
        policy_delete_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(())
    }

    pub async fn get_recording_policy(&self, id: i64) -> Result<Option<crate::domain::recording_policy::RecordingPolicy>> {
        let rows = policy_select_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().next().and_then(|r| crate::domain::recording_policy::RecordingPolicy::try_from(r).ok()))
    }

    pub async fn list_recording_policies(&self) -> Result<Vec<crate::domain::recording_policy::RecordingPolicy>> {
        let rows = policy_select_all(&self.rb)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingPolicy::try_from(r).ok()).collect())
    }

    pub async fn list_recording_policies_by_device_tag(&self, device_tag: &str) -> Result<Vec<crate::domain::recording_policy::RecordingPolicy>> {
        let rows = policy_select_by_device_tag(&self.rb, device_tag)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingPolicy::try_from(r).ok()).collect())
    }

    pub async fn list_alarm_triggered_policies(&self, alarm_type: &str) -> Result<Vec<crate::domain::recording_policy::RecordingPolicy>> {
        let rows = policy_select_by_alarm_triggered(&self.rb, alarm_type)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingPolicy::try_from(r).ok()).collect())
    }

    pub async fn create_recording_time_window(&self, window: &crate::domain::recording_policy::TimeWindow) -> Result<i64> {

        let days_literal = format!(
            "{{{}}}",
            window.days.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",")
        );

        let result = time_window_insert(
            &self.rb,
            window.policy_id,
            &window.start_time,
            &window.end_time,
            &days_literal,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(result.last_insert_id.as_i64().unwrap_or(0))
    }

    pub async fn delete_recording_time_windows(&self, policy_id: i64) -> Result<()> {
        time_window_delete_by_policy_id(&self.rb, policy_id)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(())
    }

    pub async fn list_recording_time_windows(&self, policy_id: i64) -> Result<Vec<crate::domain::recording_policy::TimeWindow>> {
        let rows = time_window_select_by_policy_id(&self.rb, policy_id)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::TimeWindow::try_from(r).ok()).collect())
    }

    pub async fn create_recording_segment(&self, segment: &crate::domain::recording_policy::RecordingSegment) -> Result<i64> {
        let start_time = RbatisDateTime::from_timestamp_millis(segment.start_time.timestamp_millis());
        let end_time = RbatisDateTime::from_timestamp_millis(segment.end_time.timestamp_millis());

        let result = segment_insert(
            &self.rb,
            segment.recording_id,
            &segment.device_tag,
            &segment.channel_tag,
            start_time,
            end_time,
            segment.duration_secs as i64,
            segment.file_size as i64,
            &segment.file_path,
            segment.server_tag.as_deref(),
            &segment.segment_type.to_string(),
            segment.alarm_id,
            segment.play_url.as_deref(),
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        result.into_iter().next().map(|r| r.id)
            .ok_or_else(|| AppError::Internal("Failed to insert recording segment".to_string()))
    }

    pub async fn get_recording_segment(&self, id: i64) -> Result<Option<crate::domain::recording_policy::RecordingSegment>> {
        let rows = segment_select_by_id(&self.rb, id)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().next().and_then(|r| crate::domain::recording_policy::RecordingSegment::try_from(r).ok()))
    }

    pub async fn insert_recording_segment(
        &self,
        recording_id: Option<i64>,
        device_tag: &str,
        channel_tag: &str,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
        duration_secs: u64,
        file_size: u64,
        file_path: &str,
        server_tag: Option<&str>,
        segment_type: &str,
        alarm_id: Option<i64>,
        play_url: Option<&str>,
    ) -> Result<i64> {
        let start_rb = RbatisDateTime::from_timestamp_millis(start_time.timestamp_millis());
        let end_rb = RbatisDateTime::from_timestamp_millis(end_time.timestamp_millis());

        let result = segment_insert(
            &self.rb,
            recording_id,
            device_tag,
            channel_tag,
            start_rb,
            end_rb,
            duration_secs as i64,
            file_size as i64,
            file_path,
            server_tag,
            segment_type,
            alarm_id,
            play_url,
        )
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;

        result.into_iter().next().map(|r| r.id)
            .ok_or_else(|| AppError::Internal("Failed to insert recording segment".to_string()))
    }

    /// 录像任务的信息：外部应用拿到录像文件时需要知道它属于哪个任务
    pub async fn recording_task_infos(
        &self,
        ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, crate::domain::recording::RecordingTaskInfo>> {
        let mut out = std::collections::HashMap::new();
        for id in ids {
            if out.contains_key(id) {
                continue;
            }
            let Some(recording) = self.get_recording(*id).await? else {
                continue;
            };
            let mut info = recording.task_info();
            if let Some(alarm_id) = recording.alarm_id {
                if let Ok(Some(alarm)) = self.get_alarm(alarm_id).await {
                    info.alarm_type = Some(alarm.alarm_type);
                    info.alarm_time = Some(alarm.alarm_time);
                }
            }
            out.insert(*id, info);
        }
        Ok(out)
    }

    /// 一个录像任务下的所有文件分段
    pub async fn list_recording_segments_by_recording_id(
        &self,
        recording_id: i64,
    ) -> Result<Vec<crate::domain::recording_policy::RecordingSegment>> {
        let rows = segment_select_by_recording_id(&self.rb, recording_id)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingSegment::try_from(r).ok()).collect())
    }

    pub async fn list_recording_segments_paginated(
        &self,
        device_tag: &str,
        channel_tag: Option<String>,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
        segment_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<crate::domain::recording_policy::RecordingSegment>> {
        let start_rb = RbatisDateTime::from_timestamp_millis(start_time.timestamp_millis());
        let end_rb = RbatisDateTime::from_timestamp_millis(end_time.timestamp_millis());

        let rows = segment_select_paginated(
            &self.rb,
            device_tag,
            channel_tag.as_deref(),
            start_rb,
            end_rb,
            segment_type,
            limit as i64,
            offset as i64,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingSegment::try_from(r).ok()).collect())
    }

    pub async fn count_recording_segments(
        &self,
        device_tag: &str,
        channel_tag: Option<String>,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
        segment_type: Option<&str>,
    ) -> Result<i64> {
        let start_rb = RbatisDateTime::from_timestamp_millis(start_time.timestamp_millis());
        let end_rb = RbatisDateTime::from_timestamp_millis(end_time.timestamp_millis());

        let count = segment_count_by_device_channel_time(
            &self.rb,
            device_tag,
            channel_tag.as_deref(),
            start_rb,
            end_rb,
            segment_type,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(count)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_recording_segments_filtered_paginated(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        segment_type: Option<&str>,
        start_time: Option<chrono::DateTime<Utc>>,
        end_time: Option<chrono::DateTime<Utc>>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<crate::domain::recording_policy::RecordingSegment>> {
        let start_rb = start_time
            .map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));
        let end_rb = end_time.map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));

        let rows = segment_select_filtered_paginated(
            &self.rb,
            device_tag,
            channel_tag,
            segment_type,
            start_rb,
            end_rb,
            limit as i64,
            offset as i64,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingSegment::try_from(r).ok()).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn count_recording_segments_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        segment_type: Option<&str>,
        start_time: Option<chrono::DateTime<Utc>>,
        end_time: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64> {
        let start_rb = start_time
            .map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));
        let end_rb = end_time.map(|t| RbatisDateTime::from_timestamp_millis(t.timestamp_millis()));

        let count = segment_count_filtered(
            &self.rb,
            device_tag,
            channel_tag,
            segment_type,
            start_rb,
            end_rb,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(count)
    }

    pub async fn list_recording_policies_paginated(
        &self,
        device_tag: Option<&str>,
        policy_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<crate::domain::recording_policy::RecordingPolicy>> {
        let rows = policy_select_paginated(
            &self.rb,
            device_tag,
            policy_type,
            limit as i64,
            offset as i64,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(rows.into_iter().filter_map(|r| crate::domain::recording_policy::RecordingPolicy::try_from(r).ok()).collect())
    }

    pub async fn count_recording_policies(
        &self,
        device_tag: Option<&str>,
        policy_type: Option<&str>,
    ) -> Result<i64> {
        let count = policy_count(&self.rb, device_tag, policy_type)
            .await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        Ok(count)
    }
}

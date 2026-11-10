use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::query_service_server::QueryService;
use rustcam_drone_proto::control::{
    Alert, AlertList, AlertListRequest, CommandStatus, CommandStatusRequest, DeviceInfo,
    DeviceList, DeviceRequest, TelemetryHistory, TelemetryHistoryRequest,
};
use tonic::{Request, Response, Status};

use crate::shadow;
use crate::state::{DeviceEntry, DroneProfileRecord, DroneState};

pub struct QueryServiceImpl {
    state: DroneState,
}

impl QueryServiceImpl {
    pub fn new(state: DroneState) -> QueryServiceImpl {
        QueryServiceImpl { state }
    }

    fn device_info(
        entry: Option<&DeviceEntry>,
        profile: Option<&DroneProfileRecord>,
        drone_tag: &str,
    ) -> DeviceInfo {
        let last = entry.and_then(|e| e.last_telemetry.as_ref());
        DeviceInfo {
            drone_tag: drone_tag.to_string(),
            protocol: entry
                .map(|e| e.protocol.clone())
                .unwrap_or_else(|| "mavlink".into()),
            adapter_id: entry.map(|e| e.adapter_id.clone()).unwrap_or_default(),
            online: entry.map(|e| e.online).unwrap_or(false),
            last_seen_ms: entry.map(|e| e.last_seen_ms).unwrap_or(0),
            last_telemetry: last.map(shadow::to_proto),
            name: profile
                .map(|p| p.name.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| drone_tag.to_string()),
            model: profile.map(|p| p.model.clone()).unwrap_or_default(),
            video_stream: profile.map(|p| p.video_stream.clone()).unwrap_or_default(),
            // `autopilot` 是运行时探测到的飞控类型；档案里配置的类型另存
            // `autopilot_configured`：两者含义不同，不能互相覆盖。
            autopilot: last.map(|t| t.autopilot.clone()).unwrap_or_default(),
            armed: last.map(|t| t.armed).unwrap_or(false),
            mode: last.map(|t| t.mode.clone()).unwrap_or_default(),
            enabled: profile.map(|p| p.enabled).unwrap_or(true),
            notes: profile.map(|p| p.notes.clone()).unwrap_or_default(),
            // 链路配置取自档案：前端编辑器完全依赖列表回填，
            // 缺一个字段就存不进去（协议版本/放行未签名帧都会被当成默认值）。
            mavlink_version: profile
                .map(|p| p.mavlink_version)
                .unwrap_or(crate::state::default_mavlink_version()),
            signing_allow_unsigned: profile
                .map(|p| p.signing_allow_unsigned)
                .unwrap_or_else(crate::state::default_signing_allow_unsigned),
            autopilot_configured: profile.map(|p| p.autopilot.clone()).unwrap_or_default(),
            has_signing_secret: profile
                .map(|p| !p.signing_secret.trim().is_empty())
                .unwrap_or(false),
        }
    }

    fn all_devices(&self) -> Vec<DeviceInfo> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in self.state.profiles.iter() {
            let profile = entry.value();
            let live = self.state.devices.get(&profile.drone_tag);
            out.push(Self::device_info(
                live.as_ref().map(|v| v.value()),
                Some(profile),
                &profile.drone_tag,
            ));
            seen.insert(profile.drone_tag.clone());
        }
        for entry in self.state.devices.iter() {
            if !seen.contains(&entry.drone_tag) {
                out.push(Self::device_info(
                    Some(entry.value()),
                    None,
                    &entry.drone_tag,
                ));
            }
        }
        out.sort_by(|a, b| a.drone_tag.cmp(&b.drone_tag));
        out
    }
}

#[tonic::async_trait]
impl QueryService for QueryServiceImpl {
    async fn list_devices(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<DeviceList>, Status> {
        Ok(Response::new(DeviceList {
            devices: self.all_devices(),
        }))
    }

    async fn get_device(
        &self,
        request: Request<DeviceRequest>,
    ) -> Result<Response<DeviceInfo>, Status> {
        let drone_tag = request.into_inner().drone_tag;
        let profile = self
            .state
            .profiles
            .get(&drone_tag)
            .map(|p| p.value().clone());
        let live = self.state.devices.get(&drone_tag).map(|d| d.clone());
        if profile.is_none() && live.is_none() {
            return Err(Status::not_found(format!("unknown device `{drone_tag}`")));
        }
        Ok(Response::new(Self::device_info(
            live.as_ref(),
            profile.as_ref(),
            &drone_tag,
        )))
    }

    async fn get_telemetry(
        &self,
        request: Request<DeviceRequest>,
    ) -> Result<Response<proto::Telemetry>, Status> {
        let drone_tag = request.into_inner().drone_tag;
        let Some(entry) = self.state.devices.get(&drone_tag) else {
            return Err(Status::not_found(format!("unknown device `{drone_tag}`")));
        };
        match &entry.last_telemetry {
            Some(t) => Ok(Response::new(shadow::to_proto(t))),
            None => Err(Status::not_found(format!(
                "no telemetry received for `{drone_tag}` yet"
            ))),
        }
    }

    async fn get_command_status(
        &self,
        request: Request<CommandStatusRequest>,
    ) -> Result<Response<CommandStatus>, Status> {
        let cmd_id = request.into_inner().cmd_id;
        match self.state.commands.get(&cmd_id) {
            Some(record) => Ok(Response::new(CommandStatus {
                cmd_id: record.cmd_id.clone(),
                status: record.status.clone(),
                message: record.message.clone(),
                drone_tag: record.drone_tag.clone(),
            })),
            None => Ok(Response::new(CommandStatus {
                cmd_id,
                status: "unknown".into(),
                message: "command not found".into(),
                drone_tag: String::new(),
            })),
        }
    }

    async fn list_alerts(
        &self,
        request: Request<AlertListRequest>,
    ) -> Result<Response<AlertList>, Status> {
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            100
        } else {
            req.limit as usize
        };
        let alerts = self
            .state
            .recent_alerts(&req.drone_tag, limit)
            .into_iter()
            .map(|a| Alert {
                drone_tag: a.drone_tag,
                rule: a.rule,
                level: a.level,
                message: a.message,
                ts_ms: a.ts_ms,
            })
            .collect();
        Ok(Response::new(AlertList { alerts }))
    }

    async fn get_telemetry_history(
        &self,
        request: Request<TelemetryHistoryRequest>,
    ) -> Result<Response<TelemetryHistory>, Status> {
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            5_000
        } else {
            req.limit as usize
        };
        // 时序库是遥测的归档源；库里没有（或查询失败）时退回内存热数据。
        let samples = match self
            .load_archived(&req.drone_tag, req.start_ms, req.end_ms, limit)
            .await
        {
            Some(list) if !list.is_empty() => list,
            _ => self
                .state
                .telemetry_history(&req.drone_tag, req.start_ms, req.end_ms, limit),
        };
        let samples = samples.iter().map(shadow::to_proto).collect();
        Ok(Response::new(TelemetryHistory { samples }))
    }
}

impl QueryServiceImpl {
    /// 从时序库读取遥测；未配置数据库时返回 `None`。
    async fn load_archived(
        &self,
        drone_tag: &str,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> Option<Vec<rustcam_drone_core::telemetry::Telemetry>> {
        let db = self.state.db.clone()?;
        match db.load_telemetry(drone_tag, start_ms, end_ms, limit).await {
            Ok(list) => Some(list),
            Err(e) => {
                tracing::warn!(error = %e, "[Drone] telemetry archive read failed");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use crate::state::{AdapterEntry, DroneProfileRecord};
    use std::sync::atomic::AtomicI64;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn state_with_device() -> DroneState {
        let state = DroneState::new(DroneConfig::default());
        let (tx, _rx) = mpsc::channel(8);
        state.adapters.insert(
            "a-1".into(),
            AdapterEntry {
                tx,
                generation: 1,
                protocol: "mavlink".into(),
                connected_at_ms: 1,
                last_heartbeat_ms: Arc::new(AtomicI64::new(1)),
            },
        );
        state
            .device_to_adapter
            .insert("drone-001".into(), "a-1".into());
        state.devices.insert(
            "drone-001".into(),
            crate::state::DeviceEntry::new("drone-001", "mavlink", "a-1", 1),
        );
        state
    }

    #[tokio::test]
    async fn list_and_get_device_return_shadow() {
        let svc = QueryServiceImpl::new(state_with_device());
        let list = svc
            .list_devices(Request::new(proto::Empty {}))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(list.devices.len(), 1);
        assert_eq!(list.devices[0].drone_tag, "drone-001");
        assert!(list.devices[0].online);

        let info = svc
            .get_device(Request::new(DeviceRequest {
                drone_tag: "drone-001".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(info.adapter_id, "a-1");
    }

    #[tokio::test]
    async fn profile_without_live_device_is_listed_offline() {
        let state = state_with_device();
        state
            .profiles
            .insert("drone-002".into(), DroneProfileRecord::new("drone-002", 1));
        let svc = QueryServiceImpl::new(state);
        let list = svc
            .list_devices(Request::new(proto::Empty {}))
            .await
            .unwrap()
            .into_inner();
        let d2 = list
            .devices
            .iter()
            .find(|d| d.drone_tag == "drone-002")
            .unwrap();
        assert!(!d2.online);
        assert_eq!(d2.name, "drone-002");
    }

    #[tokio::test]
    async fn device_info_carries_profile_link_config() {
        // 档案里配置的飞控类型/协议版本/签名策略必须随设备列表一起返回：
        // 前端编辑器完全依赖列表回填，缺一个字段就存不进去。
        let state = state_with_device();
        state.profiles.insert(
            "drone-001".into(),
            DroneProfileRecord {
                drone_tag: "drone-001".into(),
                autopilot: "px4".into(),
                mavlink_version: 1,
                signing_allow_unsigned: false,
                signing_secret: "v1:00112233".into(),
                ..DroneProfileRecord::new("drone-001", 1)
            },
        );
        let svc = QueryServiceImpl::new(state);
        let profile = svc.state.profiles.get("drone-001").unwrap().value().clone();
        let info = QueryServiceImpl::device_info(None, Some(&profile), "drone-001");
        assert_eq!(info.autopilot_configured, "px4");
        assert_eq!(info.mavlink_version, 1);
        assert!(!info.signing_allow_unsigned);
        assert!(info.has_signing_secret);
    }

    #[tokio::test]
    async fn device_info_without_profile_uses_domain_defaults() {
        // 没有档案的运行时设备：按平台默认值回填，别把"未登记"当成"关掉"。
        let svc = QueryServiceImpl::new(state_with_device());
        let info = QueryServiceImpl::device_info(None, None, "drone-x");
        assert_eq!(info.mavlink_version, 2);
        assert!(info.signing_allow_unsigned);
        assert!(!info.has_signing_secret);
        assert!(info.autopilot_configured.is_empty());
    }

    #[tokio::test]
    async fn alerts_are_returned_from_buffer() {
        let state = state_with_device();
        state.push_alert(crate::state::AlertRecord {
            drone_tag: "drone-001".into(),
            rule: "geofence_breach".into(),
            level: "high".into(),
            message: "越界".into(),
            ts_ms: 1,
            ..Default::default()
        });
        let svc = QueryServiceImpl::new(state);
        let alerts = svc
            .list_alerts(Request::new(AlertListRequest {
                drone_tag: "drone-001".into(),
                limit: 10,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(alerts.alerts.len(), 1);
        assert_eq!(alerts.alerts[0].rule, "geofence_breach");
    }

    #[tokio::test]
    async fn telemetry_requires_at_least_one_frame() {
        let svc = QueryServiceImpl::new(state_with_device());
        let err = svc
            .get_telemetry(Request::new(DeviceRequest {
                drone_tag: "drone-001".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn telemetry_history_is_returned_in_time_order() {
        let state = state_with_device();
        for ts in [1_000_i64, 2_000, 3_000] {
            state.record_telemetry(&rustcam_drone_core::telemetry::Telemetry {
                drone_tag: "drone-001".into(),
                ts_ms: ts,
                lat: 31.0,
                lon: 121.0,
                alt_amsl: 50.0,
                ..Default::default()
            });
        }
        let svc = QueryServiceImpl::new(state);
        let history = svc
            .get_telemetry_history(Request::new(TelemetryHistoryRequest {
                drone_tag: "drone-001".into(),
                start_ms: 1_500,
                end_ms: 0,
                limit: 10,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(history.samples.len(), 2);
        assert_eq!(history.samples[0].ts_ms, 2_000);
        assert_eq!(history.samples[1].ts_ms, 3_000);
    }
}

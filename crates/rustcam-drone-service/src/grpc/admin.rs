use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::admin_service_server::AdminService;
use rustcam_drone_proto::control::{
    Adapter, AdapterList, AdapterRequest, AuditList, AuditListRequest, AuditRecord as ProtoAudit,
    DeleteReply, DeviceRequest, DroneProfile, DroneProfileList, ListAdaptersRequest, Route,
    RouteDeleteRequest, RouteExecuteRequest, RouteList, RouteListRequest, SetAdapterEnabledRequest,
};
use tonic::{Request, Response, Status};

use crate::admin;
use crate::state::DroneState;

pub struct AdminServiceImpl {
    state: DroneState,
    /// 用于在链路参数变化后重算并下发分配（保存即生效）。
    registry: crate::registry::Registry,
}

impl AdminServiceImpl {
    pub fn new(state: DroneState) -> AdminServiceImpl {
        let registry = crate::registry::Registry::new(state.clone(), state.redis.clone());
        AdminServiceImpl { state, registry }
    }

    /// 通知受影响的 adapter 重算并下发分配（保存即生效）。
    ///
    /// `previous_adapter` 是变更前的归属，`current_adapter` 是变更后的归属；
    /// 归属变更时两者都要通知——旧 adapter 要关链，新 adapter 要建链。
    async fn notify_link_changed(&self, previous_adapter: &str, current_adapter: &str) {
        let mut targets: Vec<String> = Vec::new();
        for id in [previous_adapter, current_adapter] {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            if !targets.iter().any(|t| t == id) {
                targets.push(id.to_string());
            }
        }
        for adapter_id in targets {
            self.registry.resync_and_push(&adapter_id).await;
        }
    }
}

#[tonic::async_trait]
impl AdminService for AdminServiceImpl {
    async fn list_drone_profiles(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<DroneProfileList>, Status> {
        let profiles = admin::list_profiles(&self.state)
            .iter()
            .map(admin::profile_to_proto)
            .collect();
        Ok(Response::new(DroneProfileList { profiles }))
    }

    async fn upsert_drone_profile(
        &self,
        request: Request<DroneProfile>,
    ) -> Result<Response<DroneProfile>, Status> {
        let mut profile = request.into_inner();
        let operator = if profile.operator.trim().is_empty() {
            "admin".to_string()
        } else {
            profile.operator.clone()
        };
        // 旧归属必须在 upsert_profile 覆盖内存档案之前取到。
        let previous_adapter = admin::previous_adapter_id(&self.state, &profile.drone_tag);
        // 未指定链路端点时，按 adapter 的端口段自动分配（端口 + sysid）。
        admin::assign_link_slot(&self.state, &mut profile)
            .await
            .map_err(Status::invalid_argument)?;
        let record = admin::upsert_profile(&self.state, profile, &operator)
            .map_err(Status::invalid_argument)?;
        // 档案落库；链路参数以 drone 表为唯一真相源（身份 / 端点 / 密钥 / 唯一标识）
        admin::persist_link_params(&self.state, &record)
            .await
            .map_err(Status::internal)?;
        // 链路参数已落库 → 重算并下发，adapter 立即生效（保存即生效）。
        self.notify_link_changed(&previous_adapter, &record.adapter_id)
            .await;
        Ok(Response::new(admin::profile_to_proto(&record)))
    }


    async fn delete_drone_profile(
        &self,
        request: Request<DeviceRequest>,
    ) -> Result<Response<DeleteReply>, Status> {
        let drone_tag = request.into_inner().drone_tag;
        let previous_adapter = admin::previous_adapter_id(&self.state, &drone_tag);
        let deleted = admin::delete_profile(&self.state, &drone_tag, "admin")
            .await
            .map_err(Status::internal)?;
        // 档案没了 → 该机要从 adapter 的分配列表里消失，adapter 关链。
        self.notify_link_changed(&previous_adapter, "").await;
        Ok(Response::new(DeleteReply {
            deleted,
            reason: if deleted {
                String::new()
            } else {
                format!("drone `{drone_tag}` not found")
            },
        }))
    }

    async fn list_routes(
        &self,
        request: Request<RouteListRequest>,
    ) -> Result<Response<RouteList>, Status> {
        let drone_tag = request.into_inner().drone_tag;
        let routes = admin::list_routes(&self.state, &drone_tag)
            .iter()
            .map(admin::route_to_proto)
            .collect();
        Ok(Response::new(RouteList { routes }))
    }

    async fn upsert_route(&self, request: Request<Route>) -> Result<Response<Route>, Status> {
        let route = request.into_inner();
        let operator = if route.operator.is_empty() {
            "admin".to_string()
        } else {
            route.operator.clone()
        };
        let record = admin::upsert_route(&self.state, route, &operator)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(admin::route_to_proto(&record)))
    }

    async fn delete_route(
        &self,
        request: Request<RouteDeleteRequest>,
    ) -> Result<Response<DeleteReply>, Status> {
        let req = request.into_inner();
        let operator = if req.operator.is_empty() {
            "admin"
        } else {
            req.operator.as_str()
        };
        let deleted = admin::delete_route(&self.state, &req.id, operator)
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(DeleteReply {
            deleted,
            reason: if deleted {
                String::new()
            } else {
                format!("route `{}` not found", req.id)
            },
        }))
    }

    async fn execute_route(
        &self,
        request: Request<RouteExecuteRequest>,
    ) -> Result<Response<proto::CommandReply>, Status> {
        let req = request.into_inner();
        let reply = admin::execute_route(
            &self.state,
            &req.route_id,
            &req.drone_tag,
            req.lateral_offset_m,
            &req.operator,
        )
        .await
        .map_err(|e| {
            // 空域/任务冲突用 409，让前端能明确提示；其他参数错误仍是 400。
            if e.contains("空域冲突") || e.contains("已有任务") {
                Status::already_exists(e)
            } else {
                Status::failed_precondition(e)
            }
        })?;
        Ok(Response::new(reply))
    }

    async fn list_audits(
        &self,
        request: Request<AuditListRequest>,
    ) -> Result<Response<AuditList>, Status> {
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            200
        } else {
            req.limit as usize
        };
        let records = self
            .state
            .recent_audits(&req.drone_tag, limit)
            .into_iter()
            .map(|a| ProtoAudit {
                ts_ms: a.ts_ms,
                drone_tag: a.drone_tag,
                category: a.category,
                action: a.action,
                operator: a.operator,
                result: a.result,
                detail: a.detail,
            })
            .collect();
        Ok(Response::new(AuditList { records }))
    }

    async fn get_adapter(
        &self,
        request: Request<AdapterRequest>,
    ) -> Result<Response<Adapter>, Status> {
        let adapter_id = request.into_inner().adapter_id;
        let row = admin::get_adapter(&self.state, &adapter_id)
            .await
            .map_err(Status::internal)?
            .ok_or_else(|| Status::not_found(format!("adapter `{adapter_id}` not found")))?;
        Ok(Response::new(admin::adapter_to_proto(&row)))
    }

    async fn list_adapters(
        &self,
        request: Request<ListAdaptersRequest>,
    ) -> Result<Response<AdapterList>, Status> {
        let include_disabled = request.into_inner().include_disabled;
        let rows = admin::list_adapters(&self.state, include_disabled)
            .await
            .map_err(Status::internal)?;
        let adapters = rows.iter().map(admin::adapter_to_proto).collect();
        Ok(Response::new(AdapterList { adapters }))
    }

    async fn upsert_adapter(
        &self,
        request: Request<Adapter>,
    ) -> Result<Response<Adapter>, Status> {
        let adapter = request.into_inner();
        let operator = if adapter.operator.trim().is_empty() {
            "admin".to_string()
        } else {
            adapter.operator.clone()
        };
        let row = admin::upsert_adapter(&self.state, adapter, &operator)
            .await
            .map_err(Status::invalid_argument)?;
        // 端点由 adapter 的 bind_addr + 端口段拼出，bind_addr 变了端点就变了。
        self.notify_link_changed("", &row.adapter_id).await;
        Ok(Response::new(admin::adapter_to_proto(&row)))
    }

    async fn set_adapter_enabled(
        &self,
        request: Request<SetAdapterEnabledRequest>,
    ) -> Result<Response<Adapter>, Status> {
        let req = request.into_inner();
        let operator = if req.operator.trim().is_empty() {
            "admin".to_string()
        } else {
            req.operator
        };
        let row = admin::set_adapter_enabled(&self.state, &req.adapter_id, req.enabled, &operator)
            .await
            .map_err(|e| {
                if e.contains("不存在") {
                    Status::not_found(e)
                } else {
                    Status::internal(e)
                }
            })?;
        Ok(Response::new(admin::adapter_to_proto(&row)))
    }
}

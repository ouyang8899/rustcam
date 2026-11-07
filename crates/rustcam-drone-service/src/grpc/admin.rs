use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::admin_service_server::AdminService;
use rustcam_drone_proto::control::{
    AuditList, AuditListRequest, AuditRecord as ProtoAudit, DeleteReply, DeviceRequest,
    DroneProfile, DroneProfileList, Route, RouteDeleteRequest, RouteExecuteRequest, RouteList,
    RouteListRequest,
};
use tonic::{Request, Response, Status};

use crate::admin;
use crate::state::DroneState;

pub struct AdminServiceImpl {
    state: DroneState,
}

impl AdminServiceImpl {
    pub fn new(state: DroneState) -> AdminServiceImpl {
        AdminServiceImpl { state }
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
        let profile = request.into_inner();
        let operator = if profile.operator.trim().is_empty() {
            "admin".to_string()
        } else {
            profile.operator.clone()
        };
        let record = admin::upsert_profile(&self.state, profile, &operator)
            .map_err(Status::invalid_argument)?;
        // 机载链路参数落 drone 表（端点 / 密钥 / 唯一标识的唯一真相源）。
        admin::persist_link_params(&self.state, &record)
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(admin::profile_to_proto(&record)))
    }

    async fn delete_drone_profile(
        &self,
        request: Request<DeviceRequest>,
    ) -> Result<Response<DeleteReply>, Status> {
        let drone_tag = request.into_inner().drone_tag;
        let deleted = admin::delete_profile(&self.state, &drone_tag, "admin")
            .await
            .map_err(Status::internal)?;
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
}

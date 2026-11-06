use rustcam_drone_proto::control::geofence_service_server::GeofenceService;
use rustcam_drone_proto::control::{
    DeleteReply, Geofence, GeofenceDeleteRequest, GeofenceList, GeofenceListRequest,
    GeofenceUpsertRequest,
};
use tonic::{Request, Response, Status};

use crate::admin;
use crate::state::DroneState;

pub struct GeofenceServiceImpl {
    state: DroneState,
}

impl GeofenceServiceImpl {
    pub fn new(state: DroneState) -> GeofenceServiceImpl {
        GeofenceServiceImpl { state }
    }
}

#[tonic::async_trait]
impl GeofenceService for GeofenceServiceImpl {
    async fn list_geofences(
        &self,
        request: Request<GeofenceListRequest>,
    ) -> Result<Response<GeofenceList>, Status> {
        let only_enabled = request.into_inner().only_enabled;
        let geofences = admin::list_fences(&self.state)
            .iter()
            .filter(|f| !only_enabled || f.enabled)
            .map(admin::fence_to_proto)
            .collect();
        Ok(Response::new(GeofenceList { geofences }))
    }

    async fn create_geofence(
        &self,
        request: Request<GeofenceUpsertRequest>,
    ) -> Result<Response<Geofence>, Status> {
        let req = request.into_inner();
        let fence = req
            .geofence
            .ok_or_else(|| Status::invalid_argument("geofence is required"))?;
        let record = admin::upsert_fence(&self.state, fence, &req.operator)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(admin::fence_to_proto(&record)))
    }

    async fn update_geofence(
        &self,
        request: Request<GeofenceUpsertRequest>,
    ) -> Result<Response<Geofence>, Status> {
        let req = request.into_inner();
        let fence = req
            .geofence
            .ok_or_else(|| Status::invalid_argument("geofence is required"))?;
        if fence.id.trim().is_empty() {
            return Err(Status::invalid_argument(
                "geofence id is required for update",
            ));
        }
        let record = admin::upsert_fence(&self.state, fence, &req.operator)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(admin::fence_to_proto(&record)))
    }

    async fn delete_geofence(
        &self,
        request: Request<GeofenceDeleteRequest>,
    ) -> Result<Response<DeleteReply>, Status> {
        let req = request.into_inner();
        let deleted = admin::delete_fence(&self.state, &req.id, &req.operator)
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(DeleteReply {
            deleted,
            reason: if deleted {
                String::new()
            } else {
                format!("geofence {} not found", req.id)
            },
        }))
    }
}

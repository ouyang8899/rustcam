use rustcam_drone_proto::control::geofence_service_server::GeofenceService;
use rustcam_drone_proto::control::{
    DeleteReply, Geofence, GeofenceDeleteRequest, GeofenceList, GeofenceListRequest,
    GeofenceUpsertRequest,
};
use tonic::{Request, Response, Status};

use crate::state::DroneState;

pub struct GeofenceServiceImpl {
    #[allow(dead_code)]
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
        _request: Request<GeofenceListRequest>,
    ) -> Result<Response<GeofenceList>, Status> {
        Ok(Response::new(GeofenceList { geofences: vec![] }))
    }

    async fn create_geofence(
        &self,
        request: Request<GeofenceUpsertRequest>,
    ) -> Result<Response<Geofence>, Status> {
        let req = request.into_inner();
        Ok(Response::new(req.geofence.unwrap_or_default()))
    }

    async fn update_geofence(
        &self,
        request: Request<GeofenceUpsertRequest>,
    ) -> Result<Response<Geofence>, Status> {
        let req = request.into_inner();
        Ok(Response::new(req.geofence.unwrap_or_default()))
    }

    async fn delete_geofence(
        &self,
        request: Request<GeofenceDeleteRequest>,
    ) -> Result<Response<DeleteReply>, Status> {
        let id = request.into_inner().id;
        Ok(Response::new(DeleteReply {
            deleted: false,
            reason: format!("geofence {id} not found"),
        }))
    }
}

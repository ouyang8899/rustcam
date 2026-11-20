pub mod handlers;
pub mod routes;

use crate::api::state::{AppState, FullState};
use crate::auth::AuthState;
use axum::Router;

pub fn create_router(state: AppState, auth_state: AuthState) -> Router {
    let full_state = FullState {
        app: state,
        auth: auth_state,
    };
    routes::create_routes(full_state)
}

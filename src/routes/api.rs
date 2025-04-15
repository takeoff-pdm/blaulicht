use crate::routes::state::AppState;

use super::GenericResponse;
use actix_web::{
    get, put,
    web::{Data, Json},
    HttpResponse,
};
use serde::{Deserialize, Serialize};

//
// BPM Functions.
//

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BPMObject {
    pub bpm: i64,
}

#[get("/api/bpm")]
pub async fn get_bpm(data: Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(BPMObject { bpm: 42 })
}

#[put("/api/bpm")]
pub async fn set_bpm(body: Json<BPMObject>, data: Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(GenericResponse::success("updated BPM"))
}

//
// Audio device functions.
//

// TODO: maybe?

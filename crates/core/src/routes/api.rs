use actix_web::{
    get,
    web::{Data, Json},
    HttpResponse,
};
use serde::{Deserialize, Serialize};

use crate::routes::{AppState, AppStateWrapper};

//
// BPM Functions.
//

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BPMObject {
    pub bpm: i64,
}

// #[get("/api/state")]
// pub async fn get_state(data: Data<AppStateWrapper>) -> HttpResponse {
//     HttpResponse::Ok().json(&*data.state)
//     // HttpResponse::Ok().json(GenericResponse::success("updated BPM"))
// }

//
// Audio device functions.
//

// TODO: maybe?

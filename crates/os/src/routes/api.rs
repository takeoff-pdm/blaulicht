
use serde::{Deserialize, Serialize};

//
// BPM Functions.
//

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BPMObject {
    pub bpm: i64,
}

// #[put("/api/bpm")]
// pub async fn set_bpm(body: Json<BPMObject>, data: Data<AppState>) -> HttpResponse {
//     HttpResponse::Ok().json(GenericResponse::success("updated BPM"))
// }

//
// Audio device functions.
//

// TODO: maybe?

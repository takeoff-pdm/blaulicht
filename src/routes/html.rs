use actix_files::NamedFile;
use actix_web::{get, Error, HttpRequest, HttpResponse};

#[get("/")]
pub async fn get_index() -> HttpResponse {
    HttpResponse::TemporaryRedirect()
        .append_header(("Location", "/dash"))
        .finish()
}

#[get("/dash")]
pub async fn get_dash(req: HttpRequest) -> Result<HttpResponse, Error> {
    Ok(NamedFile::open("./blaulicht-web/dist/html/dash.html")?.into_response(&req))
}

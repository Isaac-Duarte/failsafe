use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../failsafe-web/dist/"]
struct WebAssets;

pub async fn serve(uri: Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_owned();
    if path.is_empty() {
        path = "index.html".to_owned();
    }

    if let Some(content) = WebAssets::get(path.as_str()) {
        return asset_response(&path, content.data.into_owned());
    }

    if let Some(content) = WebAssets::get("index.html") {
        return html_response(content.data.into_owned());
    }

    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn asset_response(path: &str, data: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(data))
        .unwrap()
}

fn html_response(data: Vec<u8>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(data))
        .unwrap()
}

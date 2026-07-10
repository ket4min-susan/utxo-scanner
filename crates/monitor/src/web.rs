// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Serves the embedded Vite/React build. The whole SPA is compiled into the binary via
//! `rust-embed`, so the monitor ships as a single artifact (DESIGN §12).

use axum::{
    body::Body,
    extract::Path,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

pub async fn index() -> Response {
    serve("index.html")
}

pub async fn asset(uri: Uri, Path(path): Path<String>) -> Response {
    let _ = uri;
    if Assets::get(&path).is_some() {
        serve(&path)
    } else {
        // Unknown non-API path: hand back index.html so client-side routing works.
        serve("index.html")
    }
}

fn serve(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                Body::from(content.data.into_owned()),
            )
                .into_response()
        },
        None => (
            StatusCode::NOT_FOUND,
            "UI assets not embedded; build the web frontend (npm run build) and rebuild.",
        )
            .into_response(),
    }
}

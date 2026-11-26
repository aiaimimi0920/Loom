// Unified multi-kind and multi-platform catalog, package, and publisher routes for `loom-art-store`.
use std::path::Path;

use loom_art_store::{
    build_catalog, build_unified_catalog, read_package_zip_version,
    read_package_zip_version_sha256, register_publisher_with_id, rotate_publisher_key,
    store_verified_published_zip, PublisherRotationRequest, StorePackageKind,
};

use crate::{store_error_response, Request, Response};

pub(crate) fn handle_catalog(root: &Path) -> Response {
    let arts = match build_catalog(root) {
        Ok(arts) => arts,
        Err(error) => return store_error_response(error),
    };
    let packages = match build_unified_catalog(root) {
        Ok(packages) => packages,
        Err(error) => return store_error_response(error),
    };
    Response::json(
        200,
        serde_json::json!({
            "arts": arts,
            "packages": packages,
        }),
    )
}

pub(crate) fn handle_package_download(root: &Path, tail: &str) -> Response {
    let mut parts = tail.splitn(3, '/');
    let (Some(kind_raw), Some(id), Some(version_file)) = (parts.next(), parts.next(), parts.next())
    else {
        return Response::json(
            404,
            serde_json::json!({ "error": "package path requires /packages/<kind>/<id>/<version>.zip" }),
        );
    };
    let Some(kind) = StorePackageKind::parse_segment(kind_raw) else {
        return Response::json(
            404,
            serde_json::json!({ "error": format!("unsupported package kind `{kind_raw}`") }),
        );
    };
    if let Some(version) = version_file.strip_suffix(".zip.sha256") {
        return match read_package_zip_version_sha256(root, kind, id, version) {
            Ok(Some(bytes)) => Response::bytes(200, "text/plain; charset=utf-8", bytes),
            Ok(None) => Response::json(
                404,
                serde_json::json!({ "error": format!("package `{id}` version `{version}` not found") }),
            ),
            Err(error) => store_error_response(error),
        };
    }
    let Some(version) = version_file.strip_suffix(".zip") else {
        return Response::json(
            404,
            serde_json::json!({ "error": "versioned package must end with .zip" }),
        );
    };
    match read_package_zip_version(root, kind, id, version) {
        Ok(Some(bytes)) => Response::bytes(200, "application/zip", bytes),
        Ok(None) => Response::json(
            404,
            serde_json::json!({ "error": format!("package `{id}` version `{version}` not found") }),
        ),
        Err(error) => store_error_response(error),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterPublisherRequest {
    #[serde(default)]
    user_id: Option<String>,
    key_id: String,
    public_key: String,
}

pub(crate) fn handle_publisher_register(request: &Request, root: &Path) -> Response {
    let input = match serde_json::from_slice::<RegisterPublisherRequest>(&request.body) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("art store: invalid publisher registration JSON: {error}");
            return Response::json(
                400,
                serde_json::json!({ "error": "invalid publisher registration request" }),
            );
        }
    };
    match register_publisher_with_id(
        root,
        input.user_id.as_deref(),
        &input.key_id,
        &input.public_key,
    ) {
        Ok(publisher) => Response::json(200, serde_json::json!({ "publisher": publisher })),
        Err(error) => store_error_response(error),
    }
}

pub(crate) fn handle_publisher_rotate(request: &Request, root: &Path, user_id: &str) -> Response {
    let input = match serde_json::from_slice::<PublisherRotationRequest>(&request.body) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("art store: invalid publisher rotation JSON: {error}");
            return Response::json(
                400,
                serde_json::json!({ "error": "invalid publisher rotation request" }),
            );
        }
    };
    match rotate_publisher_key(root, user_id, &input) {
        Ok(publisher) => Response::json(200, serde_json::json!({ "publisher": publisher })),
        Err(error) => store_error_response(error),
    }
}

pub(crate) fn handle_publish(request: &Request, root: &Path) -> Response {
    if request.body.is_empty() {
        return Response::json(400, serde_json::json!({ "error": "empty publish body" }));
    }
    let declared = request
        .header("X-Art-Id")
        .or_else(|| request.header("X-Package-Id"));
    match store_verified_published_zip(root, declared, &request.body) {
        Ok(published) => Response::json(
            200,
            serde_json::json!({
                "artId": published.art_id,
                "packageId": published.art_id,
                "kind": published.kind,
                "globalId": published.global_id,
                "published": true
            }),
        ),
        Err(error) => store_error_response(error),
    }
}

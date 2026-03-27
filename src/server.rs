use crate::generator::{
    note_id_from_relative_path, WorkspaceDoc, WorkspaceTreeNode, CURRENT_VERSION_FILE_NAME,
    NOTES_DIR_NAME, WORKSPACE_VERSIONS_DIR_NAME,
};
use anyhow::{anyhow, Context, Result};
use mime_guess::from_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use urlencoding::decode;

pub fn launch_dashboard(workspace_root: PathBuf, port: u16) -> Result<()> {
    let repo_root = workspace_root
        .parent()
        .ok_or_else(|| anyhow!("Failed to determine repository root"))?
        .to_path_buf();

    if embedded_asset("/dashboard/index.html").is_none() {
        return Err(anyhow!(
            "Embedded dashboard assets are missing from this build. Rebuild the CLI after syncing cli/static/."
        ));
    }

    let address = format!("127.0.0.1:{port}");
    let server = Server::http(&address)
        .map_err(|error| anyhow!("Failed to start dashboard server on {address}: {error}"))?;
    let url = format!("http://{address}/dashboard/");

    println!("🌐 Launching dashboard at {url}");
    open::that(&url).context("Failed to open the system browser")?;
    println!("⛵ Quartermaster is serving locally. Press Ctrl+C to stop.");

    for request in server.incoming_requests() {
        if let Err(error) = handle_request(request, &workspace_root, &repo_root) {
            eprintln!("dashboard request error: {error}");
        }
    }

    Ok(())
}

fn handle_request(request: Request, workspace_root: &Path, repo_root: &Path) -> Result<()> {
    let method = request.method().clone();
    let request_url = request.url().split('?').next().unwrap_or("/");
    let decoded = decode(request_url)?.to_string();

    match (method, decoded.as_str()) {
        (Method::Get, "/") => respond_redirect(request, "/dashboard/"),
        (Method::Get, "/dashboard") | (Method::Get, "/dashboard/") => {
            respond_embedded_asset(request, "/dashboard/index.html")
        }
        (Method::Get, path) if path.starts_with("/workspace/") => {
            let relative = path.trim_start_matches("/workspace/");
            respond_workspace_file(request, workspace_root, relative)
        }
        (Method::Get, path) if path.starts_with("/repo/") => {
            let relative = path.trim_start_matches("/repo/");
            respond_scoped_file(request, repo_root, relative)
        }
        (Method::Get, "/api/workspace/notes") => handle_get_notes_snapshot(request, workspace_root),
        (Method::Post, "/api/fs/create") => handle_create_fs(request, workspace_root, repo_root),
        (Method::Get, path) => respond_embedded_request(request, path),
        _ => respond_not_found(request),
    }
}

fn handle_get_notes_snapshot(request: Request, workspace_root: &Path) -> Result<()> {
    let snapshot = collect_notes_snapshot(workspace_root)?;
    respond_json(
        request,
        StatusCode(200),
        ApiResponse {
            ok: true,
            data: Some(snapshot),
            error: None,
        },
    )
}

fn handle_create_fs(mut request: Request, workspace_root: &Path, repo_root: &Path) -> Result<()> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let payload: CreateFsRequest = serde_json::from_str(&body)?;

    let target_path = match payload.scope.as_str() {
        "workspace" => workspace_target_path(workspace_root, &payload.path)?,
        "repo" => safe_join(repo_root, &payload.path)?,
        _ => {
            return respond_json(
                request,
                StatusCode(400),
                ApiResponse::<CreateFsResponse> {
                    ok: false,
                    data: None,
                    error: Some("Unknown scope".to_string()),
                },
            )
        }
    };

    match payload.kind.as_str() {
        "folder" => {
            fs::create_dir_all(&target_path)?;
        }
        "file" => {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            if !target_path.exists() {
                fs::write(&target_path, payload.contents.unwrap_or_default())?;
            }
        }
        _ => {
            return respond_json(
                request,
                StatusCode(400),
                ApiResponse::<CreateFsResponse> {
                    ok: false,
                    data: None,
                    error: Some("Unknown kind".to_string()),
                },
            )
        }
    }

    let response = CreateFsResponse {
        path: payload.path,
        kind: payload.kind,
    };

    respond_json(
        request,
        StatusCode(200),
        ApiResponse {
            ok: true,
            data: Some(response),
            error: None,
        },
    )
}

fn respond_redirect(request: Request, location: &str) -> Result<()> {
    let header = Header::from_bytes(b"Location", location.as_bytes())
        .map_err(|_| anyhow!("Failed to build redirect header"))?;
    let response = Response::empty(StatusCode(302)).with_header(header);
    request.respond(response)?;
    Ok(())
}

fn respond_scoped_file(request: Request, root: &Path, relative: &str) -> Result<()> {
    let path = safe_join(root, relative)?;
    respond_file(request, path)
}

fn respond_workspace_file(request: Request, workspace_root: &Path, relative: &str) -> Result<()> {
    let path = workspace_target_path(workspace_root, relative)?;
    respond_file(request, path)
}

fn respond_file(request: Request, path: PathBuf) -> Result<()> {
    if !path.exists() || !path.is_file() {
        return respond_not_found(request);
    }

    let bytes = fs::read(&path)?;
    let mime = from_path(&path).first_or_octet_stream();
    let header = Header::from_bytes(b"Content-Type", mime.essence_str().as_bytes())
        .map_err(|_| anyhow!("Failed to build content type header"))?;
    let response = Response::from_data(bytes).with_header(header);
    request.respond(response)?;
    Ok(())
}

fn respond_embedded_request(request: Request, path: &str) -> Result<()> {
    if embedded_asset(path).is_some() {
        return respond_embedded_asset(request, path);
    }

    if path.starts_with("/dashboard/") {
        return respond_embedded_asset(request, "/dashboard/index.html");
    }

    respond_not_found(request)
}

fn respond_embedded_asset(request: Request, path: &str) -> Result<()> {
    let Some(asset) = embedded_asset(path) else {
        return respond_not_found(request);
    };

    let mime = from_path(path).first_or_octet_stream();
    let header = Header::from_bytes(b"Content-Type", mime.essence_str().as_bytes())
        .map_err(|_| anyhow!("Failed to build content type header"))?;
    let response = Response::from_data(asset.bytes.to_vec()).with_header(header);
    request.respond(response)?;
    Ok(())
}

fn respond_not_found(request: Request) -> Result<()> {
    let response = Response::from_string("Not found").with_status_code(StatusCode(404));
    request.respond(response)?;
    Ok(())
}

fn respond_json<T: Serialize>(
    request: Request,
    status: StatusCode,
    body: ApiResponse<T>,
) -> Result<()> {
    let content_type = Header::from_bytes(b"Content-Type", b"application/json")
        .map_err(|_| anyhow!("Failed to build JSON content type header"))?;
    let response = Response::from_string(serde_json::to_string(&body)?)
        .with_status_code(status)
        .with_header(content_type);
    request.respond(response)?;
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let decoded = decode(relative)?.to_string();
    let relative_path = Path::new(&decoded);
    let mut joined = PathBuf::from(root);

    for component in relative_path.components() {
        match component {
            Component::Normal(value) => joined.push(value),
            Component::CurDir => {}
            _ => return Err(anyhow!("Invalid path")),
        }
    }

    Ok(joined)
}

fn workspace_target_path(workspace_root: &Path, relative: &str) -> Result<PathBuf> {
    let scoped_root =
        if relative == NOTES_DIR_NAME || relative.starts_with(&format!("{NOTES_DIR_NAME}/")) {
            workspace_root.to_path_buf()
        } else {
            resolve_current_workspace_root(workspace_root)?
        };

    safe_join(&scoped_root, relative)
}

fn resolve_current_workspace_root(workspace_root: &Path) -> Result<PathBuf> {
    let current_version_path = workspace_root.join(CURRENT_VERSION_FILE_NAME);
    if !current_version_path.exists() {
        return Ok(workspace_root.to_path_buf());
    }

    let version_id = fs::read_to_string(&current_version_path)?
        .trim()
        .to_string();

    if version_id.is_empty() {
        return Ok(workspace_root.to_path_buf());
    }

    let version_root = workspace_root
        .join(WORKSPACE_VERSIONS_DIR_NAME)
        .join(version_id);
    if version_root.exists() {
        Ok(version_root)
    } else {
        Ok(workspace_root.to_path_buf())
    }
}

fn embedded_asset(path: &str) -> Option<&'static EmbeddedAsset> {
    EMBEDDED_ASSETS.iter().find(|asset| asset.path == path)
}

#[derive(Debug, Deserialize)]
struct CreateFsRequest {
    scope: String,
    kind: String,
    path: String,
    contents: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateFsResponse {
    path: String,
    kind: String,
}

#[derive(Debug, Serialize)]
struct NotesSnapshot {
    tree: WorkspaceTreeNode,
    notes: Vec<WorkspaceDoc>,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

struct EmbeddedAsset {
    path: &'static str,
    bytes: &'static [u8],
}

fn collect_notes_snapshot(workspace_root: &Path) -> Result<NotesSnapshot> {
    let notes_root = workspace_root.join(NOTES_DIR_NAME);
    fs::create_dir_all(&notes_root)?;

    let mut notes = Vec::new();
    let tree = build_notes_tree(&notes_root, workspace_root, &mut notes)?;
    notes.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(NotesSnapshot { tree, notes })
}

fn build_notes_tree(
    absolute_path: &Path,
    workspace_root: &Path,
    notes: &mut Vec<WorkspaceDoc>,
) -> Result<WorkspaceTreeNode> {
    let relative_path = normalize_path_for_workspace(absolute_path.strip_prefix(workspace_root)?);
    let name = if relative_path == NOTES_DIR_NAME {
        "Notes".to_string()
    } else {
        absolute_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("notes")
            .to_string()
    };

    let mut children = Vec::new();
    let mut entries = fs::read_dir(absolute_path)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    entries.sort();

    for entry_path in entries {
        let metadata = match fs::metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            children.push(build_notes_tree(&entry_path, workspace_root, notes)?);
            continue;
        }

        if entry_path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }

        let note_path = normalize_path_for_workspace(entry_path.strip_prefix(workspace_root)?);
        let id = note_id_from_relative_path(&note_path);
        let title = entry_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("note")
            .replace('-', " ");
        let lines_of_code = fs::read_to_string(&entry_path)
            .ok()
            .map(|content| content.lines().count());

        notes.push(WorkspaceDoc {
            id: id.clone(),
            title,
            path: note_path.clone(),
            doc_relative_path: note_path.clone(),
            description: "Workspace note".to_string(),
            file_type: "note".to_string(),
            language: Some("Markdown".to_string()),
            size: metadata.len(),
            lines_of_code,
            imports: Vec::new(),
            references: Vec::new(),
            referenced_by: Vec::new(),
            exports: Vec::new(),
        });

        children.push(WorkspaceTreeNode {
            name: entry_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("note.md")
                .to_string(),
            path: note_path.clone(),
            node_type: "file".to_string(),
            children: Vec::new(),
            page_id: Some(id),
            doc_relative_path: Some(note_path),
        });
    }

    Ok(WorkspaceTreeNode {
        name,
        path: relative_path,
        node_type: "directory".to_string(),
        children,
        page_id: None,
        doc_relative_path: Some(normalize_path_for_workspace(
            absolute_path.strip_prefix(workspace_root)?,
        )),
    })
}

fn normalize_path_for_workspace(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|value| value.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use download_api::api::{build_router, ApiOptions};
use download_api::{DownloadService, ServiceConfig};
use download_storage::Storage;
use serde::Serialize;
use tauri::State;

#[derive(Clone, Serialize)]
struct ApiConfigDto {
    base_url: String,
    token: String,
}

struct DesktopState {
    config: ApiConfigDto,
}

#[tauri::command]
fn api_config(state: State<'_, DesktopState>) -> ApiConfigDto {
    state.config.clone()
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let file = existing_file(&path)?;
    open_file(&file)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let file = existing_file(&path)?;
    reveal_file(&file)
}

fn existing_file(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("only absolute paths can be opened".into());
    }
    let canonical =
        std::fs::canonicalize(&path).map_err(|error| format!("cannot resolve path: {error}"))?;
    if !canonical.is_file() {
        return Err("the selected path is not a regular file".into());
    }
    Ok(canonical)
}

#[cfg(target_os = "windows")]
fn open_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open file: {error}"))
}

#[cfg(target_os = "macos")]
fn open_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open file: {error}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open file: {error}"))
}

#[cfg(target_os = "windows")]
fn reveal_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not reveal file: {error}"))
}

#[cfg(target_os = "macos")]
fn reveal_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .args(["-R"])
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not reveal file: {error}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_file(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "file has no parent directory".to_string())?;
    std::process::Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not reveal file: {error}"))
}

fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "NexDownload", "NexDownload")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("data"))
}

fn load_or_create_token(directory: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let path = directory.join("api-token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if existing.len() >= 16 {
            return Ok(existing);
        }
    }
    std::fs::create_dir_all(directory)?;
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    std::fs::write(&path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(token)
}

fn default_download_dir(directory: &Path) -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|path| path.join("NexDownload")))
        .unwrap_or_else(|| directory.join("downloads"))
}

pub fn run() {
    if let Err(error) = run_inner() {
        eprintln!("NexDownload desktop: {error}");
    }
}

fn run_inner() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = data_dir();
    let token = load_or_create_token(&data_dir)?;
    let downloads = default_download_dir(&data_dir);
    std::fs::create_dir_all(&downloads)?;

    let std_listener = std::net::TcpListener::bind(("127.0.0.1", 47_821u16))
        .or_else(|_| std::net::TcpListener::bind(("127.0.0.1", 0u16)))?;
    std_listener.set_nonblocking(true)?;
    let address = std_listener.local_addr()?;
    let base_url = format!("http://{address}");

    let database = data_dir.join("nexdownload.db");
    let service_token = token.clone();
    let (service, listener) = tauri::async_runtime::block_on(async move {
        let storage = Storage::open(&database).await?;
        let config = ServiceConfig {
            data_dir: data_dir.clone(),
            default_download_dir: downloads.clone(),
            api_token: service_token.clone(),
        };
        let service = DownloadService::with_native_engine(storage, config);
        service.initialize().await?;
        service.refresh_config().await?;
        let listener = tokio::net::TcpListener::from_std(std_listener)?;
        Ok::<_, Box<dyn std::error::Error>>((service, listener))
    })?;

    let router = build_router(
        service.clone(),
        &ApiOptions {
            token: token.clone(),
            frontend_dir: None,
            rate_limit_per_minute: 900,
        },
    );
    tauri::async_runtime::spawn(async move {
        let result = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
        if let Err(error) = result {
            eprintln!("NexDownload API stopped: {error}");
        }
    });

    let shutdown = service.clone();
    let state = DesktopState {
        config: ApiConfigDto { base_url, token },
    };

    let app = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![api_config, open_path, reveal_path])
        .build(tauri::generate_context!())?;

    app.run(move |_app, event| {
        if let tauri::RunEvent::Exit = event {
            tauri::async_runtime::block_on(shutdown.shutdown());
        }
    });
    Ok(())
}

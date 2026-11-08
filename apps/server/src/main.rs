use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use download_api::{ApiOptions, DownloadService, ServiceConfig};
use download_storage::Storage;
use tracing::{error, info, warn};

struct Options {
    host: String,
    port: u16,
    data_dir: PathBuf,
    frontend_dir: Option<PathBuf>,
    token: String,
    trust_proxy: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("nexdownloadd: {error}");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    download_telemetry::init("info");
    let options = parse_options()?;
    let host: IpAddr = options
        .host
        .parse()
        .map_err(|_| format!("invalid host '{}'", options.host))?;
    if !host.is_loopback() && !options.trust_proxy {
        return Err(
            "refusing to bind a non-loopback address without TLS. Terminate TLS with a trusted \
             reverse proxy in front of NexDownload, then set NEXDOWNLOAD_TRUST_PROXY=1 to confirm."
                .into(),
        );
    }
    if !host.is_loopback() && options.token.len() < 24 {
        return Err("LAN access requires a strong API token (24+ characters)".into());
    }

    tokio::fs::create_dir_all(&options.data_dir).await?;
    let database = options.data_dir.join("nexdownload.db");
    let storage = Storage::open(&database).await?;
    let download_dir = directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|path| path.join("NexDownload")))
        .unwrap_or_else(|| options.data_dir.join("downloads"));
    tokio::fs::create_dir_all(&download_dir).await?;

    let config = ServiceConfig {
        data_dir: options.data_dir.clone(),
        default_download_dir: download_dir.clone(),
        api_token: options.token.clone(),
    };
    let service = DownloadService::with_native_engine(storage, config);
    service.initialize().await?;
    service.refresh_config().await?;

    let listener = tokio::net::TcpListener::bind((host, options.port)).await?;
    let address = listener.local_addr()?;
    let frontend = options.frontend_dir.filter(|path| path.exists());
    if frontend.is_none() {
        warn!("no built frontend found; the REST API is available but the web UI is not served");
    }

    let url = format!("http://{address}");
    println!();
    println!("  NexDownload server");
    println!("  ------------------");
    println!("  UI / API : {url}");
    println!("  Data     : {}", options.data_dir.display());
    println!("  Downloads: {}", download_dir.display());
    println!("  API token: {}", options.token);
    println!();
    println!("  Add the token in the web UI when prompted. Keep it private.");
    println!();

    let api_options = ApiOptions {
        token: options.token.clone(),
        frontend_dir: frontend,
        rate_limit_per_minute: 900,
    };
    let result = download_api::api::serve(service.clone(), listener, api_options).await;
    service.shutdown().await;
    if let Err(error) = result {
        error!("server stopped: {error}");
        return Err(error.into());
    }
    info!("server stopped");
    Ok(())
}

fn parse_options() -> Result<Options, Box<dyn std::error::Error>> {
    let mut host = std::env::var("NEXDOWNLOAD_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let mut port: u16 = std::env::var("NEXDOWNLOAD_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(47_821);
    let mut data_dir = std::env::var("NEXDOWNLOAD_DATA_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);
    let mut frontend_dir = std::env::var("NEXDOWNLOAD_FRONTEND")
        .ok()
        .map(PathBuf::from);
    let mut token = std::env::var("NEXDOWNLOAD_API_TOKEN").ok();
    let mut trust_proxy = std::env::var("NEXDOWNLOAD_TRUST_PROXY")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => host = args.next().ok_or("--host requires a value")?,
            "--port" => {
                port = args
                    .next()
                    .ok_or("--port requires a value")?
                    .parse()
                    .map_err(|_| "invalid port")?
            }
            "--data-dir" => {
                data_dir = PathBuf::from(args.next().ok_or("--data-dir requires a value")?)
            }
            "--frontend" => {
                frontend_dir = Some(PathBuf::from(
                    args.next().ok_or("--frontend requires a value")?,
                ))
            }
            "--token" => token = Some(args.next().ok_or("--token requires a value")?),
            "--trust-proxy" => trust_proxy = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}' (use --help)").into()),
        }
    }

    let token = match token {
        Some(token) if token.len() >= 16 => token,
        _ => load_or_create_token(&data_dir)?,
    };

    Ok(Options {
        host,
        port,
        data_dir,
        frontend_dir,
        token,
        trust_proxy,
    })
}

fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "NexDownload", "NexDownload")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("data"))
}

fn load_or_create_token(data_dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let path = data_dir.join("api-token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if existing.len() >= 16 {
            return Ok(existing);
        }
    }
    std::fs::create_dir_all(data_dir)?;
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

fn print_help() {
    println!(
        "nexdownloadd — NexDownload headless server\n\n\
         Options:\n\
           --host <addr>        Bind address (default 127.0.0.1)\n\
           --port <port>        Port (default 47821)\n\
           --data-dir <path>    Database and token directory\n\
           --frontend <path>    Built web UI directory\n\
           --token <token>      API token (or NEXDOWNLOAD_API_TOKEN)\n\
           --trust-proxy        Allow non-loopback bind behind a TLS reverse proxy\n\n\
         Environment:\n\
           NEXDOWNLOAD_HOST, NEXDOWNLOAD_PORT, NEXDOWNLOAD_DATA_DIR,\n\
           NEXDOWNLOAD_FRONTEND, NEXDOWNLOAD_API_TOKEN, NEXDOWNLOAD_TRUST_PROXY\n"
    );
}

#[allow(dead_code)]
fn _sleep_hint() {
    let _ = Duration::from_secs(0);
}

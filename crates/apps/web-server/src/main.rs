use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use clap::Parser;
use session_manager::SessionManager;
use tracing::info;
use tracing_subscriber::EnvFilter;

use audit_agent_web::{AppState, build_app, default_events_poll_interval, default_work_dir};

fn default_events_poll_ms() -> u64 {
    default_events_poll_interval().as_millis() as u64
}

#[derive(Debug, Parser)]
#[command(name = "audit-agent-web")]
#[command(about = "Audit Agent web server (API + static frontend)")]
struct Args {
    #[arg(long, default_value_t = 3000)]
    port: u16,

    #[arg(long, default_value_os_t = default_work_dir())]
    work_dir: PathBuf,

    #[arg(long)]
    static_dir: Option<PathBuf>,

    #[arg(long)]
    cors_origin: Option<String>,

    #[arg(
        long,
        env = "AUDIT_WEB_EVENTS_POLL_MS",
        default_value_t = default_events_poll_ms(),
        value_parser = clap::value_parser!(u64).range(100..)
    )]
    events_poll_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let state = AppState {
        manager: Arc::new(SessionManager::new(args.work_dir.clone())),
        events_poll_interval: Duration::from_millis(args.events_poll_ms),
    };
    let app: Router = build_app(state, args.static_dir.clone(), args.cors_origin.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!(
        "starting audit-agent-web on http://{} (work_dir={}, static_dir={}, events_poll_ms={})",
        addr,
        args.work_dir.display(),
        args.static_dir
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "<disabled>".to_string()),
        args.events_poll_ms
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

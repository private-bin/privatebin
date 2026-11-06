use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use axum::extract::ConnectInfo;
use hyper::body::Incoming;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tower::ServiceExt as _;

use clap::{Parser, Subcommand};
use privatebin::{Config, healthcheck, purge};
use tracing_subscriber::EnvFilter;

/// Time a client gets to send the request line and headers (also bounds idle
/// keep-alive connections), so slow clients cannot pin connections open.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);
/// How long in-flight requests may finish after a shutdown signal.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(name = "privatebin", version = env!("PRIVATEBIN_VERSION"), about = "Zero-knowledge pastebin server")]
struct Cli {
    /// Configuration file (default: /etc/privatebin/config.toml, if present).
    #[arg(long, short, global = true, env = "PRIVATEBIN_CONFIG")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Clone, Copy)]
enum Command {
    /// Run the HTTP server (default).
    Serve,
    /// Probe the local server's /healthz endpoint; exit 0 if healthy.
    Healthcheck,
    /// Delete expired pastes once and exit.
    Purge,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Serve);
    let config = match Config::load(cli.config.as_deref()) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("privatebin: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Command::Healthcheck = command {
        return match healthcheck::check(config.server.bind) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("privatebin: {e}");
                ExitCode::FAILURE
            }
        };
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("PRIVATEBIN_LOG")
                .or_else(|_| EnvFilter::try_from_default_env())
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "cannot start the async runtime");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async move {
        match command {
            Command::Serve => serve(config).await,
            Command::Purge => purge_once(config).await,
            Command::Healthcheck => unreachable!("handled above"),
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let bind = config.server.bind;
    tracing::info!(
        storage = ?config.storage.backend,
        size_limit = config.main.size_limit,
        rate_limit_secs = config.traffic.limit,
        "starting privatebin {}",
        env!("PRIVATEBIN_VERSION")
    );
    let state = privatebin::build_state(config).await?;
    let purger = purge::spawn(state.clone());
    let app = privatebin::router(state);

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| format!("cannot listen on {bind}: {e}"))?;
    tracing::info!(%bind, "listening");

    let graceful = GracefulShutdown::new();
    let mut shutdown = std::pin::pin!(shutdown_signal());
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(conn) => conn,
                Err(e) => {
                    // E.g. out of file descriptors: back off instead of spinning.
                    tracing::warn!(error = %e, "cannot accept connection");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
            () = &mut shutdown => break,
        };
        let service = app
            .clone()
            .map_request(move |mut request: axum::http::Request<Incoming>| {
                request.extensions_mut().insert(ConnectInfo(peer));
                request
            });
        let connection = hyper::server::conn::http1::Builder::new()
            .timer(TokioTimer::new())
            .header_read_timeout(HEADER_READ_TIMEOUT)
            .serve_connection(TokioIo::new(stream), TowerToHyperService::new(service));
        let connection = graceful.watch(connection);
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::debug!(error = %e, "connection closed with an error");
            }
        });
    }
    drop(listener);
    tokio::select! {
        () = graceful.shutdown() => {}
        () = tokio::time::sleep(SHUTDOWN_GRACE) => {
            tracing::warn!("timed out waiting for connections to close");
        }
    }
    purger.abort();
    tracing::info!("stopped");
    Ok(())
}

async fn purge_once(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let state = privatebin::build_state(config).await?;
    let count = purge::run_once(&state).await;
    tracing::info!(count, "purge finished");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "cannot listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

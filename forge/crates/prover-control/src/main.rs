//! # Prover Control Server
//!
//! Controls Docker Compose cluster for the distributed prover system.
//!
//! ## Endpoints
//!
//! - `GET /health` - Health check
//! - `POST /cluster/start` - Start Docker Compose cluster
//! - `POST /cluster/stop` - Stop Docker Compose cluster
//! - `GET /cluster/status` - Get cluster status
//! - `GET /cluster/logs/:container` - Get logs from specific container
//! - `POST /cluster/restart/:container` - Restart specific container

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::{process::Command, sync::RwLock};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiResponse<T> {
    status: String,
    data: Option<T>,
    message: Option<String>,
}

impl<T> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            status: "success".to_string(),
            data: Some(data),
            message: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            data: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClusterStatus {
    running: bool,
    containers: Vec<ContainerStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContainerStatus {
    name: String,
    state: String,
    health: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogResponse {
    container: String,
    logs: String,
}

struct AppState {
    compose_dir: PathBuf,
}

type SharedState = Arc<RwLock<AppState>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "prover_control=debug".into()),
        )
        .init();

    info!("Starting Prover Control Server on port 9000");

    // Find docker-compose directory
    let compose_dir = std::env::var("ZELANA_COMPOSE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Default: find the project root and navigate to deploy/docker
            std::env::current_dir()
                .unwrap()
                .join("deploy")
                .join("docker")
        });

    if !compose_dir.join("docker-compose.yml").exists() {
        error!("docker-compose.yml not found at: {}", compose_dir.display());
        error!("Please set ZELANA_COMPOSE_DIR environment variable or run from project root");
    }

    info!("Docker Compose directory: {}", compose_dir.display());

    let state = Arc::new(RwLock::new(AppState { compose_dir }));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/cluster/start", post(start_cluster_handler))
        .route("/cluster/stop", post(stop_cluster_handler))
        .route("/cluster/status", get(status_handler))
        .route("/cluster/logs/:container", get(logs_handler))
        .route(
            "/cluster/restart/:container",
            post(restart_container_handler),
        )
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9000").await?;
    info!("Control server listening on http://127.0.0.1:9000");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_handler() -> Json<ApiResponse<String>> {
    Json(ApiResponse::success(
        "Control server is healthy".to_string(),
    ))
}

async fn status_handler(State(state): State<SharedState>) -> Json<ApiResponse<ClusterStatus>> {
    let app_state = state.read().await;

    // Run docker-compose ps
    let output = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(app_state.compose_dir.join("docker-compose.yml"))
        .arg("ps")
        .arg("--format")
        .arg("json")
        .output()
        .await;

    let containers = match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut containers = Vec::new();

                for line in stdout.lines() {
                    if let Ok(container) = serde_json::from_str::<serde_json::Value>(line) {
                        containers.push(ContainerStatus {
                            name: container["Name"].as_str().unwrap_or("unknown").to_string(),
                            state: container["State"].as_str().unwrap_or("unknown").to_string(),
                            health: container["Health"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
                containers
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    let status = ClusterStatus {
        running: !containers.is_empty() && containers.iter().any(|c| c.state == "running"),
        containers,
    };

    Json(ApiResponse::success(status))
}

async fn start_cluster_handler(
    State(state): State<SharedState>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let app_state = state.read().await;

    info!("Starting Docker Compose cluster...");

    let output = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(app_state.compose_dir.join("docker-compose.yml"))
        .arg("up")
        .arg("-d")
        .output()
        .await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            if output.status.success() {
                info!("Cluster started successfully");
                // Even on success, check if there are warnings
                if !stderr.is_empty() {
                    info!("Cluster startup warnings: {}", stderr);
                }
                Ok(Json(ApiResponse::success(
                    "Cluster started successfully. Check container status in a few seconds."
                        .to_string(),
                )))
            } else {
                error!(
                    "Failed to start cluster. Exit code: {:?}",
                    output.status.code()
                );
                error!("Stderr: {}", stderr);
                error!("Stdout: {}", stdout);

                // Extract useful error message
                let error_msg = if stderr.contains("unhealthy") {
                    "Some containers failed health checks. This is normal on first start - they may need to build. Try again in a minute.".to_string()
                } else if !stderr.is_empty() {
                    stderr.lines().take(5).collect::<Vec<_>>().join("; ")
                } else {
                    "Failed to start cluster. Check Docker logs for details.".to_string()
                };

                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
        Err(e) => {
            error!("Error executing docker-compose: {}", e);
            Ok(Json(ApiResponse::error(format!(
                "Error executing docker compose: {}. Is Docker running?",
                e
            ))))
        }
    }
}

async fn stop_cluster_handler(
    State(state): State<SharedState>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let app_state = state.read().await;

    info!("Stopping Docker Compose cluster...");

    let output = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(app_state.compose_dir.join("docker-compose.yml"))
        .arg("down")
        .output()
        .await;

    match output {
        Ok(output) => {
            if output.status.success() {
                info!("Cluster stopped successfully");
                Ok(Json(ApiResponse::success(
                    "Cluster stopped successfully".to_string(),
                )))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("Failed to stop cluster: {}", stderr);
                Ok(Json(ApiResponse::error(format!(
                    "Failed to stop cluster: {}",
                    stderr
                ))))
            }
        }
        Err(e) => {
            error!("Error executing docker-compose: {}", e);
            Ok(Json(ApiResponse::error(format!(
                "Error executing docker-compose: {}",
                e
            ))))
        }
    }
}

async fn logs_handler(
    State(state): State<SharedState>,
    Path(container): Path<String>,
) -> Result<Json<ApiResponse<LogResponse>>, StatusCode> {
    let app_state = state.read().await;

    let output = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(app_state.compose_dir.join("docker-compose.yml"))
        .arg("logs")
        .arg("--tail")
        .arg("20")
        .arg("--no-color")
        .arg(&container)
        .output()
        .await;

    match output {
        Ok(output) => {
            let logs = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(Json(ApiResponse::success(LogResponse { container, logs })))
        }
        Err(e) => Ok(Json(ApiResponse::error(format!(
            "Failed to get logs: {}",
            e
        )))),
    }
}

async fn restart_container_handler(
    State(state): State<SharedState>,
    Path(container): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let app_state = state.read().await;

    info!("Restarting container: {}", container);

    let output = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(app_state.compose_dir.join("docker-compose.yml"))
        .arg("restart")
        .arg(&container)
        .output()
        .await;

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(Json(ApiResponse::success(format!(
                    "Container {} restarted",
                    container
                ))))
            } else {
                Ok(Json(ApiResponse::error("Failed to restart container")))
            }
        }
        Err(e) => Ok(Json(ApiResponse::error(format!(
            "Error restarting container: {}",
            e
        )))),
    }
}

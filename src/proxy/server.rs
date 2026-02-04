use anyhow::Result;
use axum::{
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use super::jobs::JobManager;
use super::routes;
use crate::config::settings::Settings;

/// State shared across all request handlers
pub struct AppState {
    pub job_manager: Arc<RwLock<JobManager>>,
    pub max_workers: usize,
}

/// HTTP proxy server for GIS application integration
pub struct ProxyServer {
    port: u16,
    max_workers: usize,
}

impl ProxyServer {
    pub fn new(port: u16, max_workers: usize) -> Self {
        Self { port, max_workers }
    }

    pub async fn run(&self) -> Result<()> {
        // Update settings with service info
        let mut settings = Settings::load()?;
        settings.service_port = Some(self.port);
        settings.max_workers = Some(self.max_workers);
        settings.save()?;

        // Create shared state
        let state = Arc::new(AppState {
            job_manager: Arc::new(RwLock::new(JobManager::new(self.max_workers))),
            max_workers: self.max_workers,
        });

        // Start job processor in background
        let job_manager = state.job_manager.clone();
        tokio::spawn(async move {
            loop {
                {
                    let mut manager = job_manager.write().await;
                    if let Err(e) = manager.process_pending().await {
                        tracing::error!("Job processing error: {}", e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        });

        // Build router
        let app = Router::new()
            // Health check
            .route("/api/health", get(routes::health))
            // Jobs
            .route("/api/jobs", get(routes::list_jobs))
            .route("/api/jobs", post(routes::submit_job))
            .route("/api/jobs/:id", get(routes::get_job))
            .route("/api/jobs/:id", delete(routes::cancel_job))
            .route("/api/jobs/:id/output", get(routes::get_job_output))
            // Projects
            .route("/api/projects", get(routes::list_projects))
            .route("/api/projects/:name", get(routes::get_project))
            .route("/api/projects/:name/tools", get(routes::get_project_tools))
            // Middleware
            .layer(TraceLayer::new_for_http())
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
            .with_state(state);

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        tracing::info!("GeoEngine proxy server listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

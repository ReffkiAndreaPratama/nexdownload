//! NexDownload application layer: service facade and authenticated REST API.

pub mod api;
pub mod error;
pub mod service;

pub use api::{build_router, serve, ApiErrorBody, ApiOptions};
pub use error::ServiceError;
pub use service::{
    AddResult, BenchmarkResult, Diagnostics, DownloadService, DuplicateGroup, NewDownload,
    ServiceConfig, ServiceEvent,
};

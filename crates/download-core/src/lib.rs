//! NexDownload domain core.
//!
//! This crate is deliberately free of I/O. It contains the task model, the
//! download state machine, validation rules and path-safety primitives shared
//! by every NexDownload component.

pub mod category;
pub mod error;
pub mod model;
pub mod state;
pub mod validation;

pub use category::{builtin_categories, classify_extension, Category};
pub use error::CoreError;
pub use model::{
    DownloadSegment, DownloadStatus, DownloadTask, EngineOptions, ProgressSnapshot, ResourceInfo,
    SegmentPlan,
};
pub use state::TransitionError;
pub use validation::{
    filename_from_content_disposition, filename_from_url, parse_and_validate_url, safe_destination,
    sanitize_filename, unique_path, validate_sha256,
};

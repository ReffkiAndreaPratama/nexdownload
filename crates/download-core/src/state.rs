use thiserror::Error;

use crate::model::DownloadStatus;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid state transition from {from} to {to}")]
pub struct TransitionError {
    pub from: String,
    pub to: String,
}

/// Return `true` when `to` is a legal successor of `from`.
///
/// No-op transitions are handled by [`transition`] and are always accepted.
pub fn can_transition(from: DownloadStatus, to: DownloadStatus) -> bool {
    use DownloadStatus::*;
    if from == to {
        return true;
    }
    matches!(
        (from, to),
        (Queued, Preparing)
            | (Queued, Cancelled)
            | (Preparing, Connecting)
            | (Preparing, Failed)
            | (Preparing, Cancelled)
            | (Preparing, Paused)
            | (Preparing, WaitingForNetwork)
            | (Connecting, Downloading)
            | (Connecting, Failed)
            | (Connecting, Cancelled)
            | (Connecting, Paused)
            | (Connecting, WaitingForNetwork)
            | (Downloading, Pausing)
            | (Downloading, Paused)
            | (Downloading, Verifying)
            | (Downloading, Failed)
            | (Downloading, Cancelled)
            | (Downloading, WaitingForNetwork)
            | (Pausing, Paused)
            | (Pausing, Cancelled)
            | (Pausing, Failed)
            | (Paused, Queued)
            | (Paused, Preparing)
            | (Paused, Cancelled)
            | (Verifying, Completed)
            | (Verifying, Failed)
            | (Verifying, Cancelled)
            | (Completed, Queued)
            | (Failed, Queued)
            | (Failed, Preparing)
            | (Failed, Cancelled)
            | (Cancelled, Queued)
            | (Cancelled, Preparing)
            | (WaitingForNetwork, Queued)
            | (WaitingForNetwork, Preparing)
            | (WaitingForNetwork, Paused)
            | (WaitingForNetwork, Cancelled)
            | (WaitingForNetwork, Failed)
    )
}

/// Validate and apply a state transition.
pub fn transition(
    from: DownloadStatus,
    to: DownloadStatus,
) -> Result<DownloadStatus, TransitionError> {
    if can_transition(from, to) {
        Ok(to)
    } else {
        Err(TransitionError {
            from: from.as_str().to_string(),
            to: to.as_str().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DownloadStatus::*;

    #[test]
    fn happy_path_is_allowed() {
        let path = [
            Queued,
            Preparing,
            Connecting,
            Downloading,
            Verifying,
            Completed,
        ];
        for pair in path.windows(2) {
            assert!(
                can_transition(pair[0], pair[1]),
                "{:?} -> {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn completed_cannot_jump_to_downloading() {
        assert!(!can_transition(Completed, Downloading));
        assert!(transition(Completed, Downloading).is_err());
    }

    #[test]
    fn queued_cannot_skip_preparation() {
        assert!(!can_transition(Queued, Downloading));
    }

    #[test]
    fn pause_cancel_and_retry_paths_exist() {
        assert!(can_transition(Downloading, Pausing));
        assert!(can_transition(Pausing, Paused));
        assert!(can_transition(Paused, Queued));
        assert!(can_transition(Failed, Queued));
        assert!(can_transition(Cancelled, Preparing));
    }

    #[test]
    fn no_op_is_accepted() {
        assert_eq!(transition(Paused, Paused), Ok(Paused));
    }
}

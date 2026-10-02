//! Startup stage checklist fed by [`SystemMessage::StartupStage`](crate::msg::SystemMessage).

use strum::{EnumCount, IntoEnumIterator};

use crate::msg::{StageState, StageStatus, StartupStage};

/// What the navbar status indicator shows once the init popup is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupIndicator {
    Failed,
    Restarting,
    Running,
}

pub struct StartupProgress {
    stages: [StageStatus; StartupStage::COUNT],
}

impl Default for StartupProgress {
    fn default() -> Self {
        Self {
            stages: std::array::from_fn(|_| StageStatus::PENDING),
        }
    }
}

impl StartupProgress {
    pub fn apply(&mut self, stage: StartupStage, status: StageStatus) {
        self.stages[stage as usize] = status;
    }

    pub fn iter(&self) -> impl Iterator<Item = (StartupStage, &StageStatus)> {
        StartupStage::iter().zip(self.stages.iter())
    }

    /// Most urgent non-ok state, `None` once every stage is ok.
    pub fn indicator(&self) -> Option<StartupIndicator> {
        let has = |state| self.stages.iter().any(|status| status.state == state);
        if has(StageState::Failed) {
            Some(StartupIndicator::Failed)
        } else if has(StageState::Restarting) {
            Some(StartupIndicator::Restarting)
        } else if has(StageState::Running) || has(StageState::Pending) {
            Some(StartupIndicator::Running)
        } else {
            None
        }
    }

    /// One line per stage that is not ok, for the indicator tooltip.
    pub fn summary(&self) -> String {
        self.iter()
            .filter(|(_, status)| status.state != StageState::Ok)
            .map(|(stage, status)| {
                let state = match status.state {
                    StageState::Pending => "pending",
                    StageState::Running => "running",
                    StageState::Ok => "ok",
                    StageState::Failed => "failed",
                    StageState::Restarting => "restarting",
                };
                match &status.detail {
                    Some(detail) => format!("{}: {state} ({detail})", stage.label()),
                    None => format!("{}: {state}", stage.label()),
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_ok() -> StartupProgress {
        let mut progress = StartupProgress::default();
        for stage in StartupStage::iter() {
            progress.apply(stage, StageStatus::ok(""));
        }
        progress
    }

    #[test]
    fn fresh_progress_is_running() {
        assert_eq!(
            StartupProgress::default().indicator(),
            Some(StartupIndicator::Running)
        );
    }

    #[test]
    fn all_ok_has_no_indicator() {
        let progress = all_ok();
        assert_eq!(progress.indicator(), None);
        assert_eq!(progress.summary(), "");
    }

    #[test]
    fn failed_beats_restarting_beats_running() {
        let mut progress = all_ok();
        progress.apply(StartupStage::Audio, StageStatus::running(""));
        assert_eq!(progress.indicator(), Some(StartupIndicator::Running));
        progress.apply(StartupStage::MainLoop, StageStatus::restarting(""));
        assert_eq!(progress.indicator(), Some(StartupIndicator::Restarting));
        progress.apply(
            StartupStage::Plugins,
            StageStatus::failed("1/2 · failed: x"),
        );
        assert_eq!(progress.indicator(), Some(StartupIndicator::Failed));
        assert_eq!(
            progress.summary(),
            "Audio: running\nPlugins: failed (1/2 · failed: x)\nMain loop: restarting"
        );
    }

    #[test]
    fn restart_sequence_ends_without_indicator() {
        let mut progress = all_ok();
        progress.apply(StartupStage::MainLoop, StageStatus::failed("crashed"));
        progress.apply(StartupStage::MainLoop, StageStatus::restarting(""));
        for stage in [
            StartupStage::Audio,
            StartupStage::Plugins,
            StartupStage::DmxEngine,
        ] {
            progress.apply(stage, StageStatus::PENDING);
        }
        assert_eq!(progress.indicator(), Some(StartupIndicator::Restarting));
        for stage in [
            StartupStage::Plugins,
            StartupStage::DmxEngine,
            StartupStage::Audio,
            StartupStage::MainLoop,
        ] {
            progress.apply(stage, StageStatus::ok(""));
        }
        assert_eq!(progress.indicator(), None);
    }
}

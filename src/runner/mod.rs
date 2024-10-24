use crate::host::{RunID, Host, HostInfo, RunDirectory};
use crate::payload::{PayloadInfo, PayloadMapping};
use default::DefaultRunner;
use tempfile::NamedTempFile;

pub mod default;

#[derive(serde::Serialize)]
pub struct RunnerInfo {
    cmdline: String,
}

pub trait Runner {
    fn create_run_script(&self, run_info: &RunInfo) -> NamedTempFile;

    fn run(&self, host: &dyn Host, run_dir: &RunDirectory, run_id: &RunID);

    fn cmdline(&self) -> &Vec<String>;

    fn info(&self) -> RunnerInfo {
        RunnerInfo {
            cmdline: self.cmdline().join(" "),
        }
    }
}

pub fn build_runner(cmdline: &Vec<String>) -> Box<dyn Runner> {
    Box::new(DefaultRunner::new(cmdline))
}

pub struct RunInfo {
    pub id: RunID,
    pub host: HostInfo,
    pub runner: RunnerInfo,
    pub payload: PayloadInfo,
}

impl RunInfo {
    pub fn new(
        host: &dyn Host,
        runner: &dyn Runner,
        payload_mapping: &PayloadMapping,
        run_id: &RunID,
    ) -> RunInfo {
        RunInfo {
            id: run_id.clone(),
            host: host.info(),
            runner: runner.info(),
            payload: PayloadInfo::new(
                payload_mapping,
                &host.config_dir_destination_path(&run_id),
            ),
        }
    }
}

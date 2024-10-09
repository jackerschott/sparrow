use crate::cfg::RunnerID;
use crate::host::{ExperimentID, Host, HostInfo, RunDirectory};
use crate::payload::{PayloadInfo, PayloadSource};
use snakemake::Snakemake;
use tempfile::NamedTempFile;

pub mod snakemake;

#[derive(serde::Serialize)]
pub struct RunnerInfo {
    cmdline: String,
}

pub trait Runner {
    fn create_run_script(&self, experiment_info: &ExperimentInfo) -> NamedTempFile;

    fn run(&self, host: &dyn Host, run_dir: &RunDirectory, experiment_id: &ExperimentID);

    fn cmdline(&self) -> &Vec<String>;

    fn info(&self) -> RunnerInfo {
        RunnerInfo {
            cmdline: self.cmdline().join(" "),
        }
    }
}

pub fn build_runner(id: RunnerID, cmdline: &Vec<String>) -> Box<dyn Runner> {
    match id {
        RunnerID::Snakemake => Box::new(Snakemake::new(cmdline)),
    }
}

pub struct ExperimentInfo {
    pub id: ExperimentID,
    pub host: HostInfo,
    pub runner: RunnerInfo,
    pub payload: PayloadInfo,
}

impl ExperimentInfo {
    pub fn new(
        host: &dyn Host,
        runner: &dyn Runner,
        payload_source: &PayloadSource,
        experiment_id: &ExperimentID,
    ) -> ExperimentInfo {
        ExperimentInfo {
            id: experiment_id.clone(),
            host: host.info(),
            runner: runner.info(),
            payload: PayloadInfo::new(
                payload_source,
                &host.config_dir_destination_path(&experiment_id),
            ),
        }
    }
}

use crate::cfg::RunnerID;
use crate::host::{ExperimentID, Host, HostInfo, RunDirectory};
use crate::payload::{PayloadSource, PayloadSourceInfo};
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
    pub payload_source: PayloadSourceInfo,
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
            payload_source: payload_source.info(),
        }
    }
}

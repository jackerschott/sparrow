use crate::cfg::RunnerConfig;
use crate::host::{Host, HostInfo, RunDirectory, RunID};
use crate::payload::{PayloadInfo, PayloadMapping};
use camino::Utf8PathBuf as PathBuf;
use default::DefaultRunner;
use std::collections::HashMap;
use tempfile::NamedTempFile;

pub mod default;

#[derive(serde::Serialize)]
pub struct RunnerInfo {
    cmdline: String,
    config: HashMap<String, String>,
}

pub trait Runner {
    fn create_run_script(&self, run_info: &RunInfo) -> NamedTempFile;

    fn run(&self, host: &dyn Host, run_dir: &RunDirectory, run_id: &RunID);

    fn cmdline(&self) -> &Vec<String>;
    fn config(&self) -> &HashMap<String, String>;

    fn info(&self) -> RunnerInfo {
        RunnerInfo {
            cmdline: self.cmdline().join(" "),
            config: self.config().clone(),
        }
    }
}

pub fn build_runner(
    cmdline: &Vec<String>,
    config: Option<RunnerConfig>,
) -> Box<dyn Runner> {
    let config = config.unwrap_or_default();

    let variable_transfer_requests = config
        .environment_variable_transfer_requests
        .unwrap_or(Vec::new());

    variable_transfer_requests.iter().for_each(|variable_name| {
        if let Err(err) = std::env::var(variable_name) {
            eprintln!(
                "refusing to run; \
                    expected {variable_name} to be retreivable from \
                    the local environment because of a transfer request: {err}"
            );
            std::process::exit(1);
        }
    });

    Box::new(DefaultRunner::new(
        cmdline,
        &variable_transfer_requests,
        &config.config.unwrap_or(HashMap::new()),
    ))
}

pub struct RunInfo {
    pub id: RunID,
    pub host: HostInfo,
    pub runner: RunnerInfo,
    pub payload: PayloadInfo,
    pub output_path: PathBuf,
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
            payload: PayloadInfo::new(payload_mapping, &host.config_dir_destination_path(&run_id)),
            output_path: run_id.path(host.output_base_dir_path()),
        }
    }
}

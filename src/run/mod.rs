use crate::cfg::RunnerConfig;
use crate::host::{build_host, build_local_host, Host, HostInfo, RunDirectory, RunID};
use crate::payload::{build_payload_mapping, CodeSource, PayloadInfo, PayloadMapping};
use crate::GlobalConfig;
use anyhow::{Context, Result};
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

pub fn build_runner(cmdline: &Vec<String>, config: Option<RunnerConfig>) -> Box<dyn Runner> {
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

fn print_run_script(run_script: tempfile::NamedTempFile) {
    println!("------ run_script start ------");
    std::fs::copy(run_script.path(), "/dev/stdout")
        .expect("expected copying of run script to succeed");
    println!();
    println!("------- run_script end -------");
}
pub fn run(
    run_name: String,
    run_group: Option<String>,
    config_dir: Option<PathBuf>,
    use_previous_config: bool,
    ignore_revisions: Vec<String>,
    host: String,
    enforce_quick: bool,
    no_config_review: bool,
    remainder: Vec<String>,
    only_print_run_script: bool,
    config: GlobalConfig,
) -> Result<()> {
    let run_group = run_group.unwrap_or(config.run_group);
    let run_id = RunID::new(&run_name, &run_group);

    let local_host = build_local_host(&config.local_host);

    println!("Connect to host...");
    let host = build_host(
        &host,
        &config.local_host,
        &config.remote_hosts,
        enforce_quick,
    )
    .context(format!("failed to build {host} as host"))?;

    let runner = build_runner(&remainder, config.runner);

    let config_dir = use_previous_config
        .then(|| {
            host.download_config_dir(
                &local_host,
                &RunID::new(run_name.clone(), run_group.clone()),
            )
            .context(format!(
                "failed to download {run_group}/{run_name} config directory"
            ))
        })
        .transpose()?
        .or(config_dir);
    let payload_mapping =
        build_payload_mapping(&config.payload, config_dir.as_deref(), &ignore_revisions)
            .context("failed to build payload mapping")?;

    let run_info = RunInfo::new(&*host, &*runner, &payload_mapping, &run_id);
    let run_script = runner.create_run_script(&run_info);
    if only_print_run_script {
        print_run_script(run_script);
        return Ok(());
    }

    println!(
        "Copying config to run directory from `{}'...",
        payload_mapping.config_source.dir_path
    );
    host.prepare_config_directory(
        &payload_mapping.config_source,
        &run_id,
        payload_mapping
            .code_mappings
            .iter()
            .filter_map(|code_mapping| {
                code_mapping
                    .source
                    .git_revision()
                    .map(|revision| (code_mapping.id.clone(), revision.clone()))
            })
            .collect(),
        !no_config_review,
    );

    println!("Copying code to run directory from...");
    payload_mapping
        .code_mappings
        .iter()
        .for_each(|code_mapping| {
            println!(
                "    {}: {}",
                code_mapping.id,
                match code_mapping.source {
                    CodeSource::Local { ref path, .. } => format!("{}", path),
                    CodeSource::Remote {
                        ref url,
                        ref git_revision,
                    } => format!("{}@{}", url, git_revision),
                }
            );
        });
    let run_dir = host.prepare_run_directory(
        &payload_mapping.code_mappings,
        &payload_mapping.auxiliary_mappings,
        run_script,
    );

    println!("Execute run...");
    Ok(runner.run(&*host, &run_dir, &run_id))
}

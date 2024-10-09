use clap::{Parser, Subcommand, ValueEnum};

use camino::Utf8PathBuf as PathBuf;
use serde::Deserialize;
use url::Url;

#[derive(Deserialize, Clone, Copy)]
pub enum RunnerID {
    Snakemake,
}

#[derive(Deserialize)]
pub struct RunnerConfig {
    pub experiment_group: String,
    pub runner: RunnerID,
    pub code_source: CodeSourceConfig,
    pub remote_host: RemoteHostConfig,
    pub local_host: LocalHostConfig,
    pub experiment_sync_options: ExperimentSyncOptions,
}

#[derive(Deserialize)]
pub struct LocalCodeSourceConfig {
    pub path: PathBuf,
    pub excludes: Vec<String>,
}

#[derive(Deserialize)]
pub struct RemoteCodeSourceConfig {
    pub url: Url,
}

#[derive(Deserialize)]
pub struct ConfigCodeSourceConfig {
    pub dir: PathBuf,
    pub entrypoint: PathBuf,
}

#[derive(Deserialize)]
pub struct CodeSourceConfig {
    pub local: LocalCodeSourceConfig,
    pub remote: RemoteCodeSourceConfig,
    pub config: ConfigCodeSourceConfig,
}

#[derive(Deserialize)]
pub struct QuickRunConfig {
    pub time: String,
    pub cpu_count: u16,
    pub gpu_count: u16,
    pub fast_access_container_requests: Vec<PathBuf>,
}

#[derive(Deserialize)]
pub struct RemoteHostConfig {
    pub id: String,
    pub hostname: String,
    pub experiment_base_dir: PathBuf,
    pub temporary_dir: PathBuf,
    pub quick_run: QuickRunConfig,
}

#[derive(Deserialize)]
pub struct LocalHostConfig {
    pub experiment_base_dir: PathBuf,
}

#[derive(Deserialize)]
pub struct ExperimentSyncOptions {
    pub result_excludes: Vec<String>,
    pub model_excludes: Vec<String>,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(long)]
    pub print_completion: bool,

    #[command(subcommand)]
    pub command: Option<RunnerCommandConfig>,
}

#[derive(Deserialize, ValueEnum, Clone, Debug, PartialEq)]
pub enum HostType {
    Local,
    Remote,
}

#[derive(Deserialize, ValueEnum, Clone, Debug, PartialEq)]
pub enum ExperimentSyncContent {
    Results,
    Models,
}

#[derive(Subcommand)]
pub enum RunnerCommandConfig {
    Run {
        #[arg(short = 'n', long)]
        experiment_name: String,

        #[arg(short = 'g', long)]
        experiment_group: Option<String>,

        #[arg(short = 'c', long)]
        config: Option<PathBuf>,

        #[arg(short = 'd', long, requires = "config")]
        config_dir: Option<PathBuf>,

        #[arg(short = 'v', long)]
        revision: Option<String>,

        #[arg(short = 'p', long, value_enum, default_value = "local")]
        host: HostType,

        #[arg(short = 'q', long)]
        enforce_quick: bool,

        #[arg(long)]
        no_config_review: bool,

        #[arg(trailing_var_arg = true)]
        remainder: Vec<String>,

        #[arg(long)]
        only_print_run_script: bool,
    },
    RemotePrepareQuickRun {
        #[arg(short = 't', long)]
        time: Option<String>,

        #[arg(short = 'c', long)]
        cpu_count: Option<u16>,

        #[arg(short = 'g', long)]
        gpu_count: Option<u16>,
    },
    RemoteClearQuickRun {},
    ListExperiments {
        #[arg(short = 'p', long, value_enum, default_value = "remote")]
        host: HostType,

        #[arg(short = 'r', long)]
        running: bool,
    },
    ExperimentAttach {
        #[arg(short = 'q', long)]
        quick: bool,
    },
    ExperimentSync {
        #[arg(short = 'c', long, value_enum, default_value = "results")]
        content: ExperimentSyncContent,
    },
    ExperimentLog {
        #[arg(short = 'p', long, value_enum, default_value = "remote")]
        host: HostType,

        #[arg(short = 'q', long)]
        quick_run: bool,

        #[arg(short = 'f', long)]
        follow: bool,
    },
}

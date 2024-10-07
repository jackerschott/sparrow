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
    pub main: PathBuf,
    pub paths: Vec<PathBuf>,
}

#[derive(Deserialize)]
pub struct CodeSourceConfig {
    pub local: LocalCodeSourceConfig,
    pub remote: RemoteCodeSourceConfig,
    pub config: ConfigCodeSourceConfig,
}

#[derive(Deserialize)]
pub struct RemoteHostConfig {
    pub id: String,
    pub hostname: String,
    pub experiment_base_dir: PathBuf,
    pub temporary_dir: PathBuf,
    pub fast_access_container_requests: Vec<PathBuf>,
}

#[derive(Deserialize)]
pub struct LocalHostConfig {
    pub experiment_base_dir: PathBuf,
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

#[derive(Subcommand)]
pub enum RunnerCommandConfig {
    Run {
        #[arg(short = 'n', long)]
        experiment_name: String,

        #[arg(short = 'g', long)]
        experiment_group: Option<String>,

        #[arg(short = 'v', long)]
        revision: Option<String>,

        #[arg(short = 'p', long, value_enum, default_value = "local")]
        host: HostType,

        #[arg(short = 'q', long)]
        enforce_quick: bool,

        #[arg(short = 'c', long)]
        review_config: bool,

        #[arg(trailing_var_arg = true)]
        remainder: Vec<String>,
    },
    RemotePrepare {},
    RemoteClear {},
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
    ExperimentSync {},
    ExperimentLog {
        #[arg(short = 'p', long, value_enum, default_value = "remote")]
        host: HostType,

        #[arg(short = 'q', long)]
        quick_run: bool,

        #[arg(short = 'f', long)]
        follow: bool,
    },
}

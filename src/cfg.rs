use clap::{Parser, Subcommand, ValueEnum};

use camino::Utf8PathBuf as PathBuf;
use serde::Deserialize;
use url::Url;

#[derive(Deserialize)]
pub struct RunnerConfig {
    pub experiment_group: String,
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
pub struct CodeSourceConfig {
    pub local: LocalCodeSourceConfig,
    pub remote: RemoteCodeSourceConfig,
    pub config_dirs: Vec<String>,
}

#[derive(Deserialize)]
pub struct RemoteHostConfig {
    pub id: String,
    pub hostname: String,
    pub experiment_base_dir: PathBuf,
    pub temporary_dir: PathBuf,
}

#[derive(Deserialize)]
pub struct LocalHostConfig {
    pub experiment_base_dir: PathBuf,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long)]
    pub print_completion: bool,

    #[command(subcommand)]
    pub command: Option<RunnerCommandConfig>,
}

#[derive(Deserialize, ValueEnum, Clone, Debug)]
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

        #[arg(short, long)]
        revision: Option<String>,

        #[arg(short = 'p', long, value_enum)]
        host: HostType,

        #[arg(short, long)]
        test_on_remote: bool,

        #[arg(trailing_var_arg = true)]
        remainder: Vec<String>,
    },
    AllocateTestNode {},
    DeallocateTestNode {},
    ListExperiments {},
    AttachExperiments {},
    SyncExperiments {},
    TailLog {},
}

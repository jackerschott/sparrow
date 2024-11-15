use camino::Utf8PathBuf as PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::collections::HashMap;
use url::Url;

#[derive(Deserialize)]
pub struct GlobalConfig {
    pub run_group: String,
    pub payload: PayloadMappingConfig,
    pub remote_hosts: HashMap<String, RemoteHostConfig>,
    pub local_host: LocalHostConfig,
    pub runner: Option<RunnerConfig>,
    pub run_output: RunOutputConfig,
}

#[derive(Deserialize)]
pub struct LocalCodeSourceConfig {
    pub path: PathBuf,
    pub excludes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct RemoteCodeSourceConfig {
    pub url: Url,
}

#[derive(Deserialize)]
pub struct CodeMappingConfig {
    pub id: String,
    pub local: LocalCodeSourceConfig,
    pub remote: RemoteCodeSourceConfig,
    pub target: PathBuf,
}

#[derive(Deserialize)]
pub struct ConfigSourceConfig {
    pub dir: PathBuf,
    pub entrypoint: PathBuf,
}

#[derive(Deserialize, Clone)]
pub struct AuxiliaryMappingConfig {
    pub path: PathBuf,
    pub target: PathBuf,
    pub excludes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct PayloadMappingConfig {
    pub code: Vec<CodeMappingConfig>,
    pub config: ConfigSourceConfig,
    pub auxiliary: Option<Vec<AuxiliaryMappingConfig>>,
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
    pub hostname: String,
    pub run_output_base_dir: PathBuf,
    pub temporary_dir: PathBuf,
    pub quick_run: QuickRunConfig,
}

#[derive(Deserialize)]
pub struct LocalHostConfig {
    pub run_output_base_dir: PathBuf,
}

#[derive(Deserialize, Default)]
pub struct RunnerConfig {
    pub config: Option<HashMap<String, String>>,
    pub environment_variable_transfer_requests: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct RunOutputSyncOptions {
    pub result_excludes: Vec<String>,
    pub reproduce_excludes: Vec<String>,
}

#[derive(Deserialize)]
pub struct RunOutputConfig {
    pub sync_options: RunOutputSyncOptions,
    pub results: Vec<PathBuf>,
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
pub enum RunOutputSyncContent {
    Results,
    NecessaryForReproduction,
}

#[derive(Clone)]
pub struct RevisionItem {
    pub id: String,
    pub revision: String,
}

impl std::str::FromStr for RevisionItem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('=');
        let item = RevisionItem {
            id: parts.next().ok_or("missing id")?.to_string(),
            revision: parts.next().ok_or("missing revision")?.to_string(),
        };
        parts.next().ok_or("unexpected trailing data")?;

        return Ok(item);
    }
}

#[derive(Subcommand)]
pub enum RunnerCommandConfig {
    Run {
        #[arg(short = 'n', long)]
        run_name: String,

        #[arg(short = 'g', long)]
        run_group: Option<String>,

        #[arg(short = 'c', long)]
        config_dir: Option<PathBuf>,

        #[arg(
            short = 'v',
            long,
            help = "can be used multiple times to specify the revision of each\n\
                code source to use in the format <code_source_id>=<revision>"
        )]
        revisions: Vec<RevisionItem>,

        #[arg(
            short = 'p',
            long,
            default_value = "local",
            help = "host where to run, can be 'local' or the id of any of the\n\
                remotes defined in the configuration"
        )]
        host: String,

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
        #[arg(
            short = 'p',
            long,
            help = "host where to run, can be 'local' or the id of any of the\n\
                remotes defined in the configuration"
        )]
        host: String,

        #[arg(short = 't', long)]
        time: Option<String>,

        #[arg(short = 'c', long)]
        cpu_count: Option<u16>,

        #[arg(short = 'g', long)]
        gpu_count: Option<u16>,
    },
    RemoteClearQuickRun {
        #[arg(
            short = 'p',
            long,
            help = "host where to run, can be 'local' or the id of any of the\n\
                remotes defined in the configuration"
        )]
        host: String,
    },
    ListRuns {
        #[arg(
            short = 'p',
            long,
            default_value = "local",
            help = "host from which to list runs, can be the id of any of the\n\
                remotes defined in the configuration"
        )]
        host: String,

        #[arg(short = 'r', long)]
        running: bool,
    },
    RunAttach {
        #[arg(
            short = 'p',
            long,
            help = "host to attach to, can be the id of any of the remotes defined\n\
                in the configuration"
        )]
        host: String,

        #[arg(short = 'q', long)]
        quick: bool,
    },
    RunOutputSync {
        #[arg(
            short = 'p',
            long,
            help = "host from which to sync from, can be the id of any of the remotes\n\
                defined in the configuration"
        )]
        host: String,

        #[arg(short = 'c', long, value_enum, default_value = "results")]
        content: RunOutputSyncContent,

        #[arg(short = 'r', long)]
        show_results: bool,

        #[arg(short = 'f', long, help = "ignore .from_remote marker file")]
        force: bool,
    },
    RunLog {
        #[arg(
            short = 'p',
            long,
            help = "host from which to show log output, can be the id of any of the\n\
                remotes defined in the configuration"
        )]
        host: String,

        #[arg(short = 'q', long)]
        quick_run: bool,

        #[arg(short = 'f', long)]
        follow: bool,
    },
    ShowResults {},
}

#![feature(let_chains)]
#![allow(unused)]

mod cfg;
mod host;
mod runner;
mod utils;

//use openssh::{Session, KnownHosts};
use cfg::*;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell::Fish};
use config::{Config, File, FileFormat};
use host::local::LocalHost;
use host::slurm_cluster::SlurmClusterHost;
use host::{CodeSource, ConfigSource, Host, PayloadSource};
use runner::snakemake::run;
use futures::executor::block_on;

use std::io::Write;

fn main() {
    let cli = Cli::parse();

    let config_builder = Config::builder().add_source(File::new("run", FileFormat::Yaml));

    if cli.print_completion {
        generate(Fish, &mut Cli::command(), "runner", &mut std::io::stdout());
        return;
    }

    match cli.command {
        Some(RunnerCommandConfig::Run {
            experiment_name,
            experiment_group,
            revision,
            host,
            test_on_remote,
            remainder,
        }) => {
            let config: RunnerConfig = config_builder
                .build()
                .expect("could not build configuration")
                .try_deserialize()
                .expect("Could not deserialize configuration");

            run_full(
                experiment_name,
                experiment_group.unwrap_or(config.experiment_group),
                revision,
                host,
                test_on_remote,
                config.local_host,
                config.remote_host,
                config.code_source,
                remainder,
            );
        }
        Some(RunnerCommandConfig::AllocateTestNode {}) => {
            todo!("Allocating test node");
        }
        Some(RunnerCommandConfig::DeallocateTestNode {}) => {
            todo!("Deallocating test node");
        }
        Some(RunnerCommandConfig::ListExperiments {}) => {
            todo!("Listing experiments");
        }
        Some(RunnerCommandConfig::AttachExperiments {}) => {
            todo!("Attaching experiments");
        }
        Some(RunnerCommandConfig::SyncExperiments {}) => {
            todo!("Syncing experiments");
        }
        Some(RunnerCommandConfig::TailLog {}) => {
            todo!("Tailing log");
        }
        None => {
            println!("no command specified");
        }
    }
}

fn run_full(
    experiment_name: String,
    experiment_group: String,
    revision: Option<String>,
    host: HostType,
    test_on_remote: bool,
    local_host_config: LocalHostConfig,
    remote_host_config: RemoteHostConfig,
    code_source_config: CodeSourceConfig,
    runner_cmdline: Vec<String>,
) {
    let payload_source = if let Some(revision) = revision {
        PayloadSource {
            code_source: CodeSource::Remote {
                url: code_source_config.remote.url,
                git_revision: revision,
            },
            config_source: ConfigSource {
                base_path: code_source_config.local.path,
                config_paths: code_source_config.config_dirs,
                copy_excludes: code_source_config.local.excludes.clone(),
            },
        }
    } else {
        PayloadSource {
            code_source: CodeSource::Local {
                path: code_source_config.local.path.clone(),
                copy_excludes: code_source_config.local.excludes.clone(),
            },
            config_source: ConfigSource {
                base_path: code_source_config.local.path,
                config_paths: code_source_config.config_dirs,
                copy_excludes: code_source_config.local.excludes,
            },
        }
    };

    match host {
        HostType::Local => {
            let host = LocalHost::new(local_host_config.experiment_base_dir.as_path());
            let run_dir = host.prepare_run_directory(&payload_source);
            run(host, &experiment_name, &experiment_group);
        }
        HostType::Remote => {
            let host = SlurmClusterHost::new(
                remote_host_config.id.as_str(),
                remote_host_config.hostname.as_str(),
                remote_host_config.experiment_base_dir.as_path(),
                remote_host_config.temporary_dir.as_path(),
            );
            let run_dir = host.prepare_run_directory(&payload_source);
            run(host, &experiment_name, &experiment_group);
        }
    };
}

#![feature(let_chains)]
#![feature(exit_status_error)]
#![feature(is_none_or)]

mod cfg;
mod host;
mod payload;
mod runner;
mod utils;

use crate::utils::select_interactively;
use cfg::*;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell::Fish};
use config::{Config, File, FileFormat};
use host::{build_host, ExperimentID, QuickRunPrepOptions};
use payload::build_payload_source;
use runner::{build_runner, ExperimentInfo};

fn main() {
    let cli = Cli::parse();

    if cli.print_completion {
        generate(Fish, &mut Cli::command(), "runner", &mut std::io::stdout());
        return;
    }

    let config: RunnerConfig = Config::builder()
        .add_source(File::new("run", FileFormat::Yaml))
        .build()
        .expect("could not build configuration")
        .try_deserialize()
        .expect("Could not deserialize configuration");

    match cli.command {
        Some(RunnerCommandConfig::Run {
            experiment_name,
            experiment_group,
            config_dir,
            revision,
            host,
            enforce_quick,
            no_config_review,
            remainder,
            only_print_run_script,
        }) => {
            let experiment_group = experiment_group.unwrap_or(config.experiment_group);
            let experiment_id = ExperimentID::new(&experiment_name, &experiment_group);

            println!("Connect to host...");
            let host = build_host(host, &config.local_host, &config.remote_host, enforce_quick)
                .unwrap_or_else(|err| {
                    eprintln!("error while building host: {}", err);
                    std::process::exit(1);
                });
            let runner = build_runner(config.runner, &remainder);

            let payload_source = build_payload_source(
                &config.code_source,
                config_dir.as_deref(),
                revision.as_deref(),
            );

            let experiment_info =
                ExperimentInfo::new(&*host, &*runner, &payload_source, &experiment_id);
            let run_script = runner.create_run_script(&experiment_info);
            if only_print_run_script {
                print_run_script(run_script);
                return;
            }

            println!(
                "Preparing run directory from `{}'...",
                match payload_source.code_source {
                    payload::CodeSource::Local { ref path, .. } => format!("{}", path),
                    payload::CodeSource::Remote {
                        ref url,
                        ref git_revision,
                    } => format!("{}@{}", url, git_revision),
                }
            );
            let run_dir = host.prepare_run_directory(&payload_source.code_source, run_script);

            println!(
                "Preparing config from `{}'...",
                payload_source.config_source.dir_path
            );
            host.prepare_config_directory(
                &payload_source.config_source,
                &experiment_id,
                !no_config_review,
            );

            println!("Run experiment...");
            runner.run(&*host, &run_dir, &experiment_id);
        }
        Some(RunnerCommandConfig::RemotePrepareQuickRun {
            time,
            gpu_count,
            cpu_count,
        }) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                false,
            )
            .expect("expected host building to always succeed");
            host.prepare_quick_run(&QuickRunPrepOptions::build(
                &HostType::Remote,
                time.as_deref(),
                cpu_count,
                gpu_count,
                &config.remote_host.quick_run,
            ));
            host.wait_for_preparation();
        }
        Some(RunnerCommandConfig::RemoteClearQuickRun {}) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                false,
            )
            .expect("expected host building to always succeed");
            host.clear_preparation();
        }
        Some(RunnerCommandConfig::ListExperiments { host, running }) => {
            let host = build_host(host, &config.local_host, &config.remote_host, false)
                .expect("expected host building to always succeed");

            let experiment_ids = if running {
                host.running_experiments()
            } else {
                host.experiments()
            };

            for experiment_id in experiment_ids {
                println!("{}", experiment_id);
            }
        }
        Some(RunnerCommandConfig::ExperimentAttach { quick }) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                quick,
            )
            .expect("expected host building to always succeed");
            host.attach(select_interactively(&host.running_experiments()));
        }
        Some(RunnerCommandConfig::ExperimentSync { content }) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                false,
            )
            .expect("expected host building to always succeed");

            let sync_result = host.sync(
                select_interactively(&host.experiments()),
                &config.local_host.experiment_base_dir,
                &match &content {
                    ExperimentSyncContent::Results => host::ExperimentSyncOptions {
                        excludes: config.experiment_sync_options.result_excludes,
                    },
                    ExperimentSyncContent::Models => host::ExperimentSyncOptions {
                        excludes: config.experiment_sync_options.model_excludes,
                    },
                },
            );
            if let Err(err) = sync_result {
                eprintln!("error while syncing: {}", err);
                std::process::exit(1);
            }
        }
        Some(RunnerCommandConfig::ExperimentLog {
            host,
            quick_run,
            follow,
        }) => {
            let host = build_host(host, &config.local_host, &config.remote_host, quick_run)
                .expect("expected host building to always succeed");

            let experiment_id = select_interactively(&host.running_experiments()).clone();
            let log_file_path = select_interactively(&host.log_file_paths(&experiment_id)).clone();
            println!("------ {experiment_id}, {log_file_path} ------");
            host.tail_log(&experiment_id, &log_file_path, follow);
        }
        None => {
            eprintln!("no command specified");
            std::process::exit(1);
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

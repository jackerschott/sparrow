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
use payload::build_payload_mapping;
use runner::{build_runner, ExperimentInfo};
use utils::AsUtf8Path;

fn main() {
    let cli = Cli::parse();

    if cli.print_completion {
        generate(Fish, &mut Cli::command(), "runner", &mut std::io::stdout());
        return;
    }

    let config_path = std::env::current_dir()
        .expect("expected current directory to accessible")
        .as_utf8()
        .join("run");
    let config: RunnerConfig = Config::builder()
        .add_source(File::new("run", FileFormat::Yaml))
        .build()
        .unwrap_or_else(|err| {
            eprintln!("could not build configuration: {}", err);
            std::process::exit(1);
        })
        .try_deserialize()
        .unwrap_or_else(|err| {
            eprintln!("could not deserialize configuration: {}", err);
            std::process::exit(1);
        });

    match cli.command {
        Some(RunnerCommandConfig::Run {
            experiment_name,
            experiment_group,
            config_dir,
            revisions,
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

            let payload_mapping = build_payload_mapping(
                &config.payload,
                config_dir.as_deref(),
                &revisions
                    .iter()
                    .map(|item| (item.id.clone(), item.revision.clone()))
                    .collect(),
                config_path
                    .parent()
                    .expect("expected config path to have a parent"),
            );

            let experiment_info =
                ExperimentInfo::new(&*host, &*runner, &payload_mapping, &experiment_id);
            let run_script = runner.create_run_script(&experiment_info);
            if only_print_run_script {
                print_run_script(run_script);
                return;
            }

            println!(
                "Copying config to run directory from `{}'...",
                payload_mapping.config_source.dir_path
            );
            host.prepare_config_directory(
                &payload_mapping.config_source,
                &experiment_id,
                !no_config_review,
            );

            println!("Copying code to run directory from");
            payload_mapping
                .code_mappings
                .iter()
                .for_each(|code_mapping| {
                    println!(
                        "{}: {}...",
                        code_mapping.id,
                        match code_mapping.source {
                            payload::CodeSource::Local { ref path, .. } => format!("{}", path),
                            payload::CodeSource::Remote {
                                ref url,
                                ref git_revision,
                            } => format!("{}@{}", url, git_revision),
                        }
                    );
                });
            let run_dir = host.prepare_run_directory(&payload_mapping.code_mappings, run_script);

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
        Some(RunnerCommandConfig::ExperimentSync {
            content,
            show_results,
            force,
        }) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                false,
            )
            .expect("expected host building to always succeed");

            let experiment_id = select_interactively(&host.experiments()).clone();
            let sync_result = host.sync(
                &experiment_id,
                &config.local_host.experiment_base_dir,
                &match &content {
                    ExperimentSyncContent::Results => host::ExperimentSyncOptions {
                        excludes: config.experiment_sync_options.result_excludes,
                        ignore_from_remote_marker: force,
                    },
                    ExperimentSyncContent::Models => host::ExperimentSyncOptions {
                        excludes: config.experiment_sync_options.model_excludes,
                        ignore_from_remote_marker: force,
                    },
                },
            );
            if let Err(err) = sync_result {
                eprintln!("error while syncing: {}", err);
                std::process::exit(1);
            }

            let result_path = match (show_results, config.results.len()) {
                (false, _) => {
                    std::process::exit(0);
                }
                (true, 0) => {
                    println!(
                        "Requested results, but no results path specified in config. \
                        Consider adding 'results: [experiment/relative/path/to/results]' \
                        to the config."
                    );
                    std::process::exit(1);
                }
                (true, 1) => config.results.first().unwrap(),
                (true, _) => {
                    assert!(config.results.len() > 1);
                    select_interactively(&config.results)
                }
            };

            host::local::show_result(
                &experiment_id,
                &config.local_host.experiment_base_dir,
                result_path,
            );
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
        Some(RunnerCommandConfig::ShowResults {}) => {
            let host = build_host(
                HostType::Local,
                &config.local_host,
                &config.remote_host,
                false,
            )
            .expect("expected host building to always succeed");

            let experiment_id = select_interactively(&host.experiments()).clone();

            let result_path = match config.results.len() {
                0 => {
                    println!(
                        "Requested results, but no results path specified in config. \
                        Consider adding 'results: [experiment/relative/path/to/results]' \
                        to the config."
                    );
                    std::process::exit(1);
                }
                1 => config.results.first().unwrap(),
                _ => {
                    assert!(config.results.len() > 1);
                    select_interactively(&config.results)
                }
            };

            host::local::show_result(
                &experiment_id,
                &config.local_host.experiment_base_dir,
                result_path,
            );
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

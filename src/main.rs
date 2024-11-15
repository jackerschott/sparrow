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
use host::{build_host, HostType, QuickRunPrepOptions, RunID};
use payload::build_payload_mapping;
use runner::{build_runner, RunInfo};
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
    let config: GlobalConfig = Config::builder()
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
            run_name,
            run_group,
            config_dir,
            revisions,
            host,
            enforce_quick,
            no_config_review,
            remainder,
            only_print_run_script,
        }) => {
            let run_group = run_group.unwrap_or(config.run_group);
            let run_id = RunID::new(&run_name, &run_group);

            println!("Connect to host...");
            let host = build_host(
                &host,
                &config.local_host,
                &config.remote_hosts,
                enforce_quick,
            )
            .unwrap_or_else(|err| {
                eprintln!("error while building host: {}", err);
                std::process::exit(1);
            });
            let runner = build_runner(&remainder, config.runner);

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

            let run_info = RunInfo::new(&*host, &*runner, &payload_mapping, &run_id);
            let run_script = runner.create_run_script(&run_info);
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
                &run_id,
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
                            payload::CodeSource::Local { ref path, .. } => format!("{}", path),
                            payload::CodeSource::Remote {
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
            runner.run(&*host, &run_dir, &run_id);
        }
        Some(RunnerCommandConfig::RemotePrepareQuickRun {
            host: host_id,
            time,
            gpu_count,
            cpu_count,
        }) => {
            if host_id == "local" {
                eprintln!("Cannot prepare quick run on local host");
                std::process::exit(1);
            }

            let host = build_host(&host_id, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");
            host.prepare_quick_run(&QuickRunPrepOptions::build(
                &HostType::Remote,
                time.as_deref(),
                cpu_count,
                gpu_count,
                &config.remote_hosts[&host_id].quick_run,
            ));
            host.wait_for_preparation();
        }
        Some(RunnerCommandConfig::RemoteClearQuickRun { host }) => {
            if host == "local" {
                eprintln!("Cannot prepare quick run on local host");
                std::process::exit(1);
            }

            let host = build_host(&host, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");
            host.clear_preparation();
        }
        Some(RunnerCommandConfig::ListRuns { host, running }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");

            let run_ids = if running {
                host.running_runs()
            } else {
                host.runs()
            };

            for run_id in run_ids {
                println!("{}", run_id);
            }
        }
        Some(RunnerCommandConfig::RunAttach { host, quick }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, quick)
                .expect("expected host building to always succeed");
            host.attach(select_interactively(&host.running_runs()));
        }
        Some(RunnerCommandConfig::RunOutputSync {
            host,
            content,
            show_results,
            force,
        }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");

            let run_id = select_interactively(&host.runs()).clone();
            let sync_result = host.sync(
                &run_id,
                &config.local_host.run_output_base_dir,
                &match &content {
                    RunOutputSyncContent::Results => host::RunOutputSyncOptions {
                        excludes: config.run_output.sync_options.result_excludes,
                        ignore_from_remote_marker: force,
                    },
                    RunOutputSyncContent::NecessaryForReproduction => host::RunOutputSyncOptions {
                        excludes: config.run_output.sync_options.reproduce_excludes,
                        ignore_from_remote_marker: force,
                    },
                },
            );
            if let Err(err) = sync_result {
                eprintln!("error while syncing: {}", err);
                std::process::exit(1);
            }

            let result_path = match (show_results, config.run_output.results.len()) {
                (false, _) => {
                    std::process::exit(0);
                }
                (true, 0) => {
                    println!(
                        "Requested results, but no results path specified in config. \
                        Consider adding 'results: [output_dir/relative/path/to/results]' \
                        to the config."
                    );
                    std::process::exit(1);
                }
                (true, 1) => config.run_output.results.first().unwrap(),
                (true, _) => {
                    assert!(config.run_output.results.len() > 1);
                    select_interactively(&config.run_output.results)
                }
            };

            host::local::show_result(&run_id, &config.local_host.run_output_base_dir, result_path);
        }
        Some(RunnerCommandConfig::RunLog {
            host,
            quick_run,
            follow,
        }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, quick_run)
                .expect("expected host building to always succeed");

            let run_id = select_interactively(&host.running_runs()).clone();
            let log_file_path = select_interactively(&host.log_file_paths(&run_id)).clone();
            println!("------ {run_id}, {log_file_path} ------");
            host.tail_log(&run_id, &log_file_path, follow);
        }
        Some(RunnerCommandConfig::ShowResults {}) => {
            let host = build_host("local", &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");

            let run_id = select_interactively(&host.runs()).clone();

            let result_path = match config.run_output.results.len() {
                0 => {
                    println!(
                        "Requested results, but no results path specified in config. \
                        Consider adding 'results: [output_dir/relative/path/to/results]' \
                        to the config."
                    );
                    std::process::exit(1);
                }
                1 => config.run_output.results.first().unwrap(),
                _ => {
                    assert!(config.run_output.results.len() > 1);
                    select_interactively(&config.run_output.results)
                }
            };

            host::local::show_result(&run_id, &config.local_host.run_output_base_dir, result_path);
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

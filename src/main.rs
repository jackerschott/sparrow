#![feature(let_chains)]
#![feature(exit_status_error)]
//#![allow(unused)]

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
use host::{build_host, ExperimentID};
use payload::build_payload_source;
use runner::{build_runner, ExperimentInfo};

fn main() {
    let cli = Cli::parse();

    let config: RunnerConfig = Config::builder()
        .add_source(File::new("run", FileFormat::Yaml))
        .build()
        .expect("could not build configuration")
        .try_deserialize()
        .expect("Could not deserialize configuration");

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
            enforce_quick,
            review_config,
            remainder,
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
            let payload_source = build_payload_source(&config.code_source, revision.as_deref());

            let experiment_info =
                ExperimentInfo::new(&*host, &*runner, &payload_source, &experiment_id);
            let run_script = runner.create_run_script(&experiment_info);

            let run_dir = host.prepare_run_directory(&payload_source, run_script, review_config);

            println!("Run experiment...");
            runner.run(&*host, &run_dir, &experiment_id);
        }
        Some(RunnerCommandConfig::RemotePrepare {}) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                true,
            )
            .expect("expected host building to always succeed");
            host.prepare();
            host.wait_for_preparation();
        }
        Some(RunnerCommandConfig::RemoteClear {}) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                true,
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
        Some(RunnerCommandConfig::ExperimentSync {}) => {
            let host = build_host(
                HostType::Remote,
                &config.local_host,
                &config.remote_host,
                false,
            )
            .expect("expected host building to always succeed");

            host.sync(
                select_interactively(&host.experiments()),
                &config.local_host.experiment_base_dir,
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
            host.tail_log(&experiment_id, &log_file_path, follow);
        }
        None => {
            eprintln!("no command specified");
            std::process::exit(1);
        }
    }
}

//! A simple cli experiment submission tool compatible with slurm clusters
//! which can
//!
//! * test code and configuration locally
//! * upload experiment code and configuration from a development machine
//! * monitor the status of the experiment while it is running
//! * organize and managing experiment outputs
//!
//! In particular, sparrow can
//!
//! * enable local testing of code and configuration before running code on more involved hardware,
//!   while making the switch as easy as changing a command line flag
//! * allow starting many experiments in parallel without any unnecessary overhead or worries about
//!   code/config overlaps
//! * allow for easy config changes and reviews during submission, making it harder to accidentally
//!   use the wrong config for a 10 hour neural-network training
//! * track and pin the exact git commmit and configuration used for the training, to allow for full
//!   reproduciblity, or just use the current state of your code when you need convenience
//!
//! # Getting Started
//!
//! Build and install sparrow by cloning the [repository](https://gitlab.cern.ch/jackersc/sparrow)
//! and running
//!
//! ```shell
//! cargo install --path .
//! ```
//!
//! Note that you might need to add ~/.cargo/bin to your PATH in bashrc/zshrc/config.fish, before
//! being able to execute `sparrow`.
//!
//! Next we need to create a `run.yaml` file that contains everything sparrow needs to now about
//! your setup, i.e. mostly your code and the cluster you want to run on.
//! Consult the documentation of the [`cfg`] module for details on how to write this file.
//!
//! Now we only need to define the command we want sparrow to run our code with.
//! This is done by writing a run.sh.j2 file, which is a bash script template that uses the [jinja
//! specification](https://jinja.palletsprojects.com/en/stable/).
//! A common example using snakemake would be
//!
//! ```shell
//! {%- if not host.is_local -%}
//! module load GCCcore/13.2.0 Python/3.11.5
//!
//! {% endif -%}
//! snakemake \
//!     --snakefile=workflow/biastest.smk \
//!     --workflow-profile=workflow/profiles/{{ host.id + "-test" if host.is_configured_for_quick_run and not host.is_local else host.id }} \
//!     --keep-going \
//!     {{ runner.cmdline }} \
//!     --config \
//!         experiment_name={{ run_id.name }} \
//!         experiment_group={{ run_id.group }} \
//!         experiment_base_dir={{ host.run_output_base_dir_path }} \
//!         code_revision={{ payload.code_revisions.sourcerer }} \
//!         host={{ host.id }} \
//!         devstage={{ 'test' if host.is_local or host.is_configured_for_quick_run else 'experiment' }} \
//!         config_dir={{ payload.config_dir }} \
//!         user=jona \
//!         fast_dev_run=False
//! ```
//!
//! Everything inside of `{{` and `}}` is a jinja template expression, which will be rendered and
//! populated by sparrow to create the final run script.
//! These expression allow for some logic with a python-like syntax, like if-statements and loops.
//! The variables that jinja uses are defined and documented by sparrow in the [`RunInfo`] struct.
//!
//! To launch an experiment after both `run.yaml` and `run.sh.j2` are created, we can run
//!
//! ```shell
//! sparrow run --run-name my_experiment
//! ```
//!
//! This will simply launch the command we defined in `run.sh.j2` on our local machine in a
//! temporary run directory and point the command to the output directory we defined in `run.yaml`
//! under `<run-group>/my_experiment` (where the run group is also defined in `run.yaml`).
//!
//! If we want to launch the experiment on a remote host instead, we simply specify the id of the
//! remote host, as specified in `run.yaml`
//!
//! ```shell
//! sparrow run --host <host-id> --run-name my_experiment
//! ```
//!
//! This will copy all code and configuration to the remote machine into a dedicated run directory
//! and execute the given command in a tmux session from which one can de- and reattach.
//!
//! It is often useful to run the experiment on a remote host, but on a pre-allocated node, instead
//! of the login node.
//!
//! In this case we can simply use
//!
//! ```shell
//! sparrow remote-prepare-quick-run --host <host-id>
//! ```
//!
//! And subsequently execute the run command with the `--enforce-quick` flag and the run will
//! automatically use the pre-allocated node.
//! Note that for this to work, sparrow assumes that the pre-allocated node is accessible via ssh
//! under `<hostname>-quick`.
//! This can be done by adding the following to your ssh configuration
//!
//! ```ssh
//! Host <hostname>-quick
//!     User ackersch
//!     ProxyCommand ssh -q <hostname> 'nc $(squeue --noheader --format %%N --user <username> --name quick-run-towel) 22'
//! ```
//!
//! where `quick-run-towel` is the name `sparrow` uses to identify the job that allocates the node.
//! In addition, you also need to add your public key to `~/.ssh/authorized_keys` on the login node
//! of the cluster(s) you want to use. While the login node is configured to accept your public key
//! automatically, the compute nodes do not. So we add the key manually in our home directory which
//! is shared with the compute nodes automatically via the network file system.
//!
//! [`cfg`]: crate::cfg
//! [`RunInfo`]: crate::runner::RunInfo

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
use host::{build_host, QuickRunPrepOptions, RunID};
use payload::build_payload_mapping;
use runner::{build_runner, RunInfo};
use utils::AsUtf8Path;

fn main() {
    let cli = Cli::parse();

    if cli.print_completion {
        generate(Fish, &mut Cli::command(), "sparrow", &mut std::io::stdout());
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
            use_previous_config,
            ignore_revisions,
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

            let config_dir = use_previous_config
                .then_some(host.config_dir_destination_path(&RunID::new(run_name, run_group)))
                .or(config_dir);
            let payload_mapping = build_payload_mapping(
                &config.payload,
                config_dir.as_deref(),
                &ignore_revisions,
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
                time.as_deref(),
                cpu_count,
                gpu_count,
                &config.remote_hosts[&host_id].quick_run,
            ));
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

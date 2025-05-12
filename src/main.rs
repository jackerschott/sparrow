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
//! Next we need to create `.sparrow/config.yaml` and `.sparrow/private.yaml` files that contains
//! everything sparrow needs to now about your setup, i.e. mostly your code and the cluster you want to run on.
//! Consult the documentation of the [`cfg`] module for details on how to write this file and note
//! that the two files get merged into one configuration, where `.sparrow/private.yaml` has
//! priority.
//!
//! Now we only need to define the command we want sparrow to run our code with.
//! This is done by writing a .sparrow/run.sh.j2 file, which is a bash script template that uses the [jinja
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
//! To launch an experiment after `.sparrow/config.yaml`, `.sparrow/private.yaml` and `.sparrow/run.sh.j2`
//! are created, we can run
//!
//! ```shell
//! sparrow run --run-name my_experiment
//! ```
//!
//! This will simply launch the command we defined in `.sparrow/run.sh.j2` on our local machine in a
//! temporary run directory and point the command to the output directory we defined in the
//! configuration files under `<run-group>/my_experiment` (where the run group is also defined in the config).
//!
//! If we want to launch the experiment on a remote host instead, we simply specify the id of the
//! remote host, as specified in the configuration
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
mod run;
mod utils;

use crate::utils::select_interactively;
use anyhow::{anyhow, bail, Context, Result};
use cfg::*;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell::Fish};
use config::{Config, File, FileFormat};
use host::{build_host, QuickRunPrepOptions};
use run::run;

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.print_completion {
        generate(Fish, &mut Cli::command(), "sparrow", &mut std::io::stdout());
        return Ok(());
    }

    let config: GlobalConfig = Config::builder()
        .add_source(File::new(".sparrow/config", FileFormat::Yaml))
        .add_source(File::new(".sparrow/private", FileFormat::Yaml))
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
        }) => run(
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
            config,
        )
        .context("run failed"),
        Some(RunnerCommandConfig::RemotePrepareQuickRun {
            host: host_id,
            time,
            gpu_count,
            cpu_count,
            constraint,
        }) => {
            if host_id == "local" {
                return Err(anyhow!("cannot prepare quick run on local host"));
            }

            let host = build_host(&host_id, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");
            if host.quick_run_is_prepared().context(format!(
                "failed to check for the quick preparation of {}",
                host.id()
            ))? {
                println!("quick run is already prepared for {host}", host = host.id());
                return Ok(());
            }

            host.prepare_quick_run(&QuickRunPrepOptions::build(
                time.as_deref(),
                cpu_count,
                gpu_count,
                constraint,
                &config.remote_hosts[&host_id].quick_run,
            ))
            .context(format!("failed to prepare {} for quick runs", host.id()))
        }
        Some(RunnerCommandConfig::RemoteClearQuickRun { host }) => {
            if host == "local" {
                eprintln!("cannot prepare quick run on local host");
                std::process::exit(1);
            }

            let host = build_host(&host, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");
            host.clear_preparation();

            Ok(())
        }
        Some(RunnerCommandConfig::ListRuns { host, running }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");

            let run_ids = if running {
                host.running_runs()
            } else {
                host.runs()
                    .context(format!("failed to obtain runs from {}", host.id()))?
            };

            for run_id in run_ids {
                println!("{}", run_id);
            }

            Ok(())
        }
        Some(RunnerCommandConfig::RunAttach { host, quick }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, quick)
                .expect("expected host building to always succeed");
            host.attach(
                select_interactively(&host.running_runs(), "run: ")
                    .context("failed to select a run to attach to")?,
            );

            Ok(())
        }
        Some(RunnerCommandConfig::RunOutputSync {
            host,
            content,
            show_results,
            force,
        }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");

            let run_id = select_interactively(
                &host
                    .runs()
                    .context(format!("failed to obtain runs from {}", host.id()))?,
                "run: ",
            )
            .context("failed to select a run to synchronize")?
            .clone();
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
                    select_interactively(&config.run_output.results, "result: ")
                        .context("failed to select a result to synchronize")?
                }
            };

            host::local::show_result(&run_id, &config.local_host.run_output_base_dir, result_path);

            Ok(())
        }
        Some(RunnerCommandConfig::RunLog {
            host,
            quick_run,
            follow,
        }) => {
            let host = build_host(&host, &config.local_host, &config.remote_hosts, quick_run)
                .expect("expected host building to always succeed");

            let run_id = select_interactively(&host.running_runs(), "run: ")
                .context("failed to select a run to select a log file from")?
                .clone();
            let log_file_path = select_interactively(&host.log_file_paths(&run_id), "log: ")
                .context("failed to select a log file")?
                .clone();
            println!("------ {run_id}, {log_file_path} ------");
            host.tail_log(&run_id, &log_file_path, follow);

            Ok(())
        }
        Some(RunnerCommandConfig::ShowResults {}) => {
            let host = build_host("local", &config.local_host, &config.remote_hosts, false)
                .expect("expected host building to always succeed");

            let run_id = select_interactively(
                &host
                    .runs()
                    .context(format!("failed to obtain runs from {}", host.id()))?,
                "run: ",
            )
            .context("failed to select a run to select a result from")?
            .clone();

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
                    select_interactively(&config.run_output.results, "result: ")
                        .context("failed to select a result to show")?
                }
            };

            host::local::show_result(&run_id, &config.local_host.run_output_base_dir, result_path);

            Ok(())
        }
        None => bail!("no command specified, use --help to see available commands"),
    }
}

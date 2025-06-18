use super::connection::Connection;
use super::local::LocalHost;
use super::rsync::SyncOptions;
use super::{Host, QuickRunPrepOptions, RunDirectory, RunID, RunOutputSyncOptions};
use crate::utils::Utf8Path;
use anyhow::{anyhow, Context, Result};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use core::str;
use std::os::unix::process::CommandExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct QuickRunPreparationOptions {
    pub slurm_account: String,
    pub slurm_service_quality: Option<String>,
    pub node_local_storage_path: PathBuf,
}

pub struct SlurmClusterHost {
    id: String,
    script_run_command_template: String,
    output_base_dir_path: PathBuf,
    temporary_dir_path: PathBuf,

    hostname: String,
    connection: Connection,
    quick_run_preparation: QuickRunPreparationOptions,
}

impl SlurmClusterHost {
    const QUICK_RUN_TOWEL_JOB_NAME: &str = "quick-run-towel";

    pub fn new(
        id: &str,
        hostname: &str,
        script_run_command_template: String,
        output_base_dir_path: &Path,
        temporary_dir_path: &Path,
        quick_run_preparation: QuickRunPreparationOptions,
        allow_quick_runs: bool,
    ) -> Self {
        let hostname = if allow_quick_runs {
            &format!("{hostname}-quick")
        } else {
            hostname
        };

        let connection = match Connection::new(hostname) {
            Ok(connection) => connection,
            Err(e) => {
                eprintln!("Failed to connect to host {}: {:?}", hostname, e);
                if allow_quick_runs {
                    eprintln!("Did you forget to prepare the remote?")
                }
                std::process::exit(1);
            }
        };

        return Self {
            id: id.to_owned(),
            hostname: hostname.to_owned(),
            script_run_command_template,
            output_base_dir_path: output_base_dir_path.to_owned(),
            temporary_dir_path: temporary_dir_path.to_owned(),
            connection,
            quick_run_preparation,
        };
    }
}

impl SlurmClusterHost {
    pub fn allocate_quick_run_node(
        &self,
        constraint: &Option<String>,
        partitions: &Option<Vec<String>>,
        time: &str,
        cpu_count: u16,
        gpu_count: u16,
        fast_access_container_paths: &Vec<PathBuf>,
    ) -> Result<()> {
        let submission_script = Self::build_quick_run_towel_job_script(
            fast_access_container_paths,
            &self.quick_run_preparation.node_local_storage_path,
        );

        let submission_options = Self::quick_run_towel_job_submission_options(
            self.quick_run_preparation.slurm_account.clone(),
            self.quick_run_preparation.slurm_service_quality.clone(),
            constraint,
            partitions,
            time,
            cpu_count,
            gpu_count,
        );

        self.submit_quick_run_towel_job(&submission_script, &submission_options)
            .context("failed to submit quick run towel job")?;

        Ok(())
    }

    pub fn deallocate_quick_run_node(&self) {
        let status = self
            .connection
            .command("scancel")
            .arg("--name")
            .arg(Self::QUICK_RUN_TOWEL_JOB_NAME)
            .status()
            .expect("expected scancel to succeed");

        if !status.success() {
            panic!("expected scancel to have a successful exit code");
        }
    }

    pub fn has_allocated_quick_run_node(&self) -> Result<bool> {
        let check_command_inner = format!(
            "squeue --noheader --format %%t --user $USER --name {}",
            Self::QUICK_RUN_TOWEL_JOB_NAME
        );
        let check_command = format!("bash -c \"{check_command_inner}\"");

        let output = self
            .connection
            .command("bash")
            .arg("-c")
            .arg(check_command_inner)
            .stdout(openssh::Stdio::piped())
            .stderr(openssh::Stdio::piped())
            .output()
            .expect("expected squeue to succeed");
        if !output.status.success() {
            let error_message = String::from_utf8(output.stderr).context(format!(
                "failed to run `{check_command}' on {id} and couldn't read the \
                    error message due to a failure to convert it to utf8",
                id = self.id()
            ))?;
            eprintln!("{error_message}");

            return Err(anyhow!("failed to run `{check_command}`"));
        }

        let output = String::from_utf8(output.stdout).context(format!(
            "failed to convert the output of `{check_command}' (run on {id}) to utf8",
            id = self.id()
        ))?;
        let job_status = output.trim();

        return Ok(job_status == "R");
    }

    fn submit_quick_run_towel_job(&self, script: &str, options: &Vec<String>) -> Result<()> {
        let mut submission_command = self.connection.command("salloc");
        let submission_commmand_string =
            format!("salloc {} -- bash -c \"bash -\"", options.join(" "));
        let mut submission_command = submission_command
            .args(options)
            .arg("--")
            .arg("bash")
            .arg("-c")
            .arg(&format!("bash -"))
            .stdin(openssh::Stdio::piped())
            .stdout(openssh::Stdio::piped())
            .spawn()
            .context(format!(
                "failed to execute `{submission_commmand_string}' on {hostname}",
                hostname = self.hostname
            ))?;

        let stdin = submission_command.stdin().as_mut().context(format!(
            "failed to open stdin of `{submission_commmand_string}'"
        ))?;
        self.connection
            .block_on(stdin.write_all(script.as_bytes()))
            .context(format!(
                "failed to write to stdin of `{submission_commmand_string}'"
            ))?;

        let stdout = submission_command.stdout().as_mut().context(format!(
            "failed to open stdout of `{submission_commmand_string}'"
        ))?;

        const OUTPUT_CHUNK_COUNT_MAX: u16 = 10_000;
        const OUTPUT_CHUNK_SIZE: usize = 1_000;
        let mut output = [0u8; OUTPUT_CHUNK_SIZE];
        let output_chunks = (0..OUTPUT_CHUNK_COUNT_MAX)
            .into_iter()
            .map(|_| {
                let output_length =
                    self.connection
                        .block_on(stdout.read(&mut output))
                        .context(format!(
                            "failed to read stdout of `{submission_commmand_string}'`"
                        ))?;
                let output =
                    String::from_utf8(output[..output_length].to_vec()).context(format!(
                        "failed to convert some output of `{submission_commmand_string}' to utf8"
                    ))?;
                if !output.is_empty() {
                    println!("{output}");
                }

                Ok(output)
            })
            .take_while(|output_chunk| {
                output_chunk
                    .as_ref()
                    .map_or(false, |chunk| chunk != "Going to sleep...")
            })
            .collect::<Result<Vec<_>>>()?;
        if output_chunks.len() as u16 == OUTPUT_CHUNK_COUNT_MAX {
            return Err(anyhow!(
                "failed to read the `Going to sleep...' line using {chunk_count} \
                output chunks of size {chunk_size} indicating the success of `{command}'",
                chunk_count = OUTPUT_CHUNK_COUNT_MAX,
                chunk_size = OUTPUT_CHUNK_SIZE,
                command = submission_commmand_string
            ));
        }

        self.connection
            .block_on(submission_command.disconnect())
            .context(format!(
                "failed to disconnect from `{submission_commmand_string}'"
            ))?;

        Ok(())
    }

    fn build_quick_run_towel_job_script(
        fast_access_container_paths: &Vec<PathBuf>,
        node_local_storage_path: &Path,
    ) -> String {
        let container_copy_loop = if fast_access_container_paths.is_empty() {
            ""
        } else {
            let fast_access_container_paths = fast_access_container_paths
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<&str>>()
                .join(" ");
            &format!(
                "\
                for container_file in {fast_access_container_paths}; do\n\
                    rsync --progress $container_file {node_local_storage_path}/\n\
                done",
            )
        };

        format!(
            concat!(
                "#!/bin/bash\n",
                "{}\n",
                "printf \"Going to sleep...\"\n",
                "sleep 1d",
            ),
            container_copy_loop
        )
    }

    fn quick_run_towel_job_submission_options(
        account: String,
        quality_of_service: Option<String>,
        constraint: &Option<String>,
        partitions: &Option<Vec<String>>,
        time: &str,
        cpu_count: u16,
        gpu_count: u16,
    ) -> Vec<String> {
        let mut options = vec![format!("--account={account}")];

        if let Some(quality_of_service) = quality_of_service {
            options.push(format!("--qos={quality_of_service}"));
        }

        if let Some(partitions) = partitions {
            options.push(format!("--partition={}", partitions.join(",")))
        }

        if let Some(constraint) = constraint {
            options.push(format!("--constraint={constraint}"));
        }

        options.extend(vec![
            format!("--job-name={}", Self::QUICK_RUN_TOWEL_JOB_NAME),
            format!("--nodes=1-1"),
            format!("--time={time}"),
            format!("--cpus-per-task={cpu_count}"),
            format!("--gpus={gpu_count}"),
        ]);

        return options;
    }
}

impl Host for SlurmClusterHost {
    fn id(&self) -> &str {
        &self.id
    }
    fn hostname(&self) -> &str {
        &self.hostname
    }
    fn script_run_command(&self, script_path: &str) -> String {
        return self.script_run_command_template.replace("{}", script_path);
    }
    fn output_base_dir_path(&self) -> &Path {
        &self.output_base_dir_path.as_path()
    }
    fn is_local(&self) -> bool {
        false
    }
    fn is_configured_for_quick_run(&self) -> bool {
        self.hostname.ends_with("-quick")
    }

    fn upload_run_dir(&self, prep_dir: tempfile::TempDir) -> RunDirectory {
        let run_dir_path = self.temporary_dir_path.join(tmpname("run.", "", 4));
        self.connection.upload(
            &prep_dir.utf8_path(),
            &run_dir_path,
            SyncOptions::default().copy_contents(),
        );
        return RunDirectory::Remote(run_dir_path);
    }
    fn download_config_dir(&self, local: &LocalHost, run_id: &RunID) -> Result<PathBuf> {
        let destination_path = local.config_dir_destination_path(run_id);
        local.create_dir_all(&destination_path);
        self.connection.download(
            &self.config_dir_destination_path(run_id),
            &destination_path,
            SyncOptions::default().copy_contents(),
        );

        Ok(destination_path)
    }

    fn put(&self, local_path: &Path, host_path: &Path, options: SyncOptions) {
        self.connection.upload(local_path, host_path, options);
    }

    fn create_dir(&self, path: &Path) {
        self.connection
            .command("mkdir")
            .arg(path)
            .status()
            .expect(&format!("expected mkdir {path} to succeed"));
    }

    fn create_dir_all(&self, path: &Path) {
        self.connection
            .command("mkdir")
            .arg("-p")
            .arg(path)
            .status()
            .expect(&format!("expected mkdir {path} to succeed"));
    }

    fn prepare_quick_run(&self, options: &QuickRunPrepOptions) -> Result<()> {
        match &options {
            QuickRunPrepOptions::SlurmCluster {
                constraint,
                partitions,
                time,
                cpu_count,
                gpu_count,
                fast_access_container_paths,
            } => {
                self.allocate_quick_run_node(
                    constraint,
                    partitions,
                    &time,
                    *cpu_count,
                    *gpu_count,
                    fast_access_container_paths,
                )?;
            }
        }

        Ok(())
    }
    fn quick_run_is_prepared(&self) -> Result<bool> {
        self.has_allocated_quick_run_node()
    }

    fn clear_preparation(&self) {
        self.deallocate_quick_run_node()
    }

    fn runs(&self) -> Result<Vec<RunID>> {
        let mut find_command = self.connection.command("find");
        find_command
            .arg(self.output_base_dir_path.as_str())
            .arg("-mindepth")
            .arg("2")
            .arg("-maxdepth")
            .arg("2")
            .arg("-type")
            .arg("d");
        let find_command_string = format!("{find_command:?}");

        let find_output = find_command
            .stderr(openssh::Stdio::inherit())
            .output()
            .context(format!("failed to run `{find_command_string}`"))?;

        let find_output = String::from_utf8(find_output.stdout).unwrap();

        Ok(find_output
            .lines()
            .map(|line| Path::new(line))
            .map(|path| {
                let name = path.file_name().unwrap();
                let group = path.parent().unwrap().file_name().unwrap();
                RunID::new(name, group)
            })
            .collect())
    }
    fn running_runs(&self) -> Vec<RunID> {
        let tmux_output = self
            .connection
            .command("tmux")
            .arg("list-sessions")
            .output()
            .expect("expected run output find to succeed");

        if !tmux_output.status.success() {
            return Vec::new();
        }

        let tmux_output = String::from_utf8(tmux_output.stdout).unwrap();

        tmux_output
            .lines()
            .map(|line| line.split(":").next().unwrap())
            .map(|session_name| session_name.split("/"))
            .map(|mut parts| {
                let group = parts.next().unwrap();
                let name = parts.next().unwrap();
                assert!(parts.next().is_none());
                RunID::new(name, group)
            })
            .collect()
    }
    fn log_file_paths(&self, run_id: &RunID) -> Vec<PathBuf> {
        let log_path = run_id.path(&self.output_base_dir_path);

        let find_output = self
            .connection
            .command("find")
            .arg(log_path)
            .arg("-type")
            .arg("f")
            .arg("-name")
            .arg("*.log")
            .output()
            .expect("expected log find to succeed");

        if !find_output.status.success() {
            return Vec::new();
        }

        let find_output = String::from_utf8(find_output.stdout).unwrap();

        find_output
            .lines()
            .map(|line| Path::new(line))
            .map(|path| {
                path.strip_prefix(&run_id.path(&self.output_base_dir_path))
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }
    fn attach(&self, run_id: &RunID) {
        let err = std::process::Command::new(std::env::var("SHELL").unwrap())
            .arg("-c")
            .arg(&format!(
                "ssh -tt {} 'exec tmux attach-session -t {run_id}'",
                self.hostname
            ))
            .exec();
        panic!("expected exec to never fail: {err}");
    }
    fn sync(
        &self,
        run_id: &RunID,
        local_base_path: &Path,
        options: &RunOutputSyncOptions,
    ) -> Result<(), String> {
        let local_dest_path = run_id.path(local_base_path);
        let from_remote_marker_path = local_dest_path.join(".from_remote");

        if local_dest_path.exists()
            && !from_remote_marker_path.exists()
            && !options.ignore_from_remote_marker
        {
            return Err(format!(
                "{local_dest_path} does exist but the `.from_remote' \
                marker does not exist, refusing to sync"
            ));
        }

        if !local_dest_path.exists() {
            std::fs::create_dir_all(&local_dest_path).expect(&format!(
                "expected creation of missing {local_dest_path} components to work"
            ));
        }

        self.connection.download(
            &run_id.path(&self.output_base_dir_path),
            &local_dest_path,
            SyncOptions::default()
                .copy_contents()
                .exclude(&options.excludes)
                .progress(),
        );

        std::fs::File::create(&from_remote_marker_path).expect(&format!(
            "expected creation of {from_remote_marker_path} to work"
        ));

        Ok(())
    }
    fn tail_log(&self, run_id: &RunID, log_file_path: &Path, follow: bool) {
        let log_file_path = run_id.path(&self.output_base_dir_path).join(log_file_path);
        let cmd = if follow { "tail -Fq" } else { "cat" };
        let err = std::process::Command::new(std::env::var("SHELL").unwrap())
            .arg("-c")
            .arg(&format!(
                "ssh -tt {} 'exec {cmd} {log_file_path}'",
                self.hostname
            ))
            .exec();
        panic!("expected exec to never fail: {err}");
    }
}

fn tmpname(prefix: &str, suffix: &str, rand_len: u8) -> String {
    let rand_len = usize::from(rand_len);
    let mut name = String::with_capacity(
        prefix
            .len()
            .saturating_add(suffix.len())
            .saturating_add(rand_len),
    );
    name += prefix;
    let mut char_buf = [0u8; 4];
    for c in std::iter::repeat_with(fastrand::alphanumeric).take(rand_len) {
        name += c.encode_utf8(&mut char_buf);
    }
    name += suffix;
    name
}

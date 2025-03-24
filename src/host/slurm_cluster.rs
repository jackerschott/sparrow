use super::connection::Connection;
use super::rsync::SyncOptions;
use super::{Host, QuickRunPrepOptions, RunDirectory, RunID, RunOutputSyncOptions};
use crate::utils::Utf8Path;
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use std::os::unix::process::CommandExt;
use tokio::io::AsyncWriteExt;

pub enum QuickRun {
    Disabled,
    Enabled,
}

pub struct SlurmClusterHost {
    id: String,
    script_run_command_template: String,
    output_base_dir_path: PathBuf,
    temporary_dir_path: PathBuf,

    hostname: String,
    connection: Connection,
    quick_run_config: QuickRun,
    quick_run_service_quality: Option<String>,
}

impl SlurmClusterHost {
    const QUICK_RUN_TOWEL_JOB_NAME: &str = "quick-run-towel";

    pub fn new(
        id: &str,
        hostname: &str,
        script_run_command_template: String,
        output_base_dir_path: &Path,
        temporary_dir_path: &Path,
        quick_run_config: QuickRun,
        quick_run_service_quality: Option<String>,
    ) -> Self {
        let hostname = if let QuickRun::Enabled = quick_run_config {
            &format!("{hostname}-quick")
        } else {
            hostname
        };

        let connection = match Connection::new(hostname) {
            Ok(connection) => connection,
            Err(e) => {
                eprintln!("Failed to connect to host {}: {}", hostname, e);
                if let QuickRun::Enabled = quick_run_config {
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
            quick_run_config,
            quick_run_service_quality,
        };
    }
}

impl SlurmClusterHost {
    pub fn allocate_quick_run_node(
        &self,
        time: &str,
        cpu_count: u16,
        gpu_count: u16,
        fast_access_container_paths: &Vec<PathBuf>,
    ) {
        let submission_script = Self::build_quick_run_towel_job_script(fast_access_container_paths);

        let submission_options = Self::quick_run_towel_job_submission_options(
            self.quick_run_service_quality.clone(),
            time,
            cpu_count,
            gpu_count,
        );

        let log_path = self.quick_run_towel_log_path();
        self.submit_quick_run_towel_job(&submission_script, &submission_options, &log_path);
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

    pub fn has_allocated_quick_run_node(&self) -> bool {
        let output = self
            .connection
            .command("squeue")
            .stdout(openssh::Stdio::null())
            .args(&[
                "--noheader",
                "--format %%N",
                "--user",
                "ackersch",
                "--name",
                Self::QUICK_RUN_TOWEL_JOB_NAME,
            ])
            .output()
            .expect("expected squeue to succeed");

        let output = String::from_utf8(output.stdout).expect("expected squeue output to be utf-8");
        let node_name = output.trim();

        return !node_name.is_empty();
    }

    fn submit_quick_run_towel_job(&self, script: &str, options: &Vec<String>, log_path: &Path) {
        let mut submission_command = self.connection.command("salloc");
        let mut submission_command = submission_command
            .arg(&format!("--output={log_path}"))
            .args(options)
            .arg("bash -")
            .stdin(openssh::Stdio::piped())
            .spawn()
            .expect("expected sbatch to succeed");

        let stdin = submission_command
            .stdin()
            .as_mut()
            .expect("expected stdin to be open");
        self.connection
            .block_on(stdin.write_all(script.as_bytes()))
            .expect("expected stdin write to succeed");

        self.connection
            .block_on(submission_command.wait())
            .expect("expected sbatch to succeed");
    }

    fn build_quick_run_towel_job_script(fast_access_container_paths: &Vec<PathBuf>) -> String {
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
                    rsync --progress $container_file /scratch/\n\
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
        quality_of_service: Option<String>,
        time: &str,
        cpu_count: u16,
        gpu_count: u16,
    ) -> Vec<String> {
        let mut options = Vec::new();

        if let Some(quality_of_service) = quality_of_service {
            options.push(quality_of_service.clone())
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

    fn tail_quick_run_towel_submission_log(&self, log_path: &Path) {
        let mut tail_command = self.connection.command("tail");
        let tail_command = tail_command
            .arg("--follow=name")
            .arg("--retry")
            .arg("--quiet")
            .arg(log_path.as_str())
            .spawn()
            .expect("expected tail to succeed");

        loop {
            let output = self
                .connection
                .command("cat")
                .arg(log_path.as_str())
                .output()
                .expect("expected cat to succeed");
            if !output.status.success() {
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }

            let log_content =
                String::from_utf8(output.stdout).expect("expected cat output to be utf-8");
            let last_line = log_content
                .trim()
                .split("\n")
                .last()
                .expect("expected cat output to have at least one line");
            if last_line == "Going to sleep..." {
                break;
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        self.connection
            .block_on(tail_command.disconnect())
            .expect("expected tail disconnect to succeed");
    }

    fn quick_run_towel_log_path(&self) -> PathBuf {
        self.temporary_dir_path.join("quick-run-towel.log")
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
        return self.script_run_command_template.replace("{}", script_path)
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

    fn prepare_quick_run(&self, options: &QuickRunPrepOptions) {
        match &self.quick_run_config {
            QuickRun::Enabled => {}
            QuickRun::Disabled => match &options {
                QuickRunPrepOptions::SlurmCluster {
                    time,
                    cpu_count,
                    gpu_count,
                    fast_access_container_paths,
                } => self.allocate_quick_run_node(
                    &time,
                    *cpu_count,
                    *gpu_count,
                    fast_access_container_paths,
                ),
            },
        }
    }
    fn quick_run_is_prepared(&self) -> bool {
        match &self.quick_run_config {
            QuickRun::Enabled => true,
            QuickRun::Disabled => self.has_allocated_quick_run_node(),
        }
    }

    fn wait_for_preparation(&self) {
        match &self.quick_run_config {
            QuickRun::Enabled => {}
            QuickRun::Disabled => {
                self.tail_quick_run_towel_submission_log(&self.quick_run_towel_log_path())
            }
        }
    }

    fn clear_preparation(&self) {
        self.deallocate_quick_run_node()
    }

    fn runs(&self) -> Vec<RunID> {
        let find_output = self
            .connection
            .command("find")
            .arg(self.output_base_dir_path.as_str())
            .arg("-mindepth")
            .arg("2")
            .arg("-maxdepth")
            .arg("2")
            .arg("-type")
            .arg("d")
            .output()
            .expect("expected run output find to succeed");

        if !find_output.status.success() {
            return Vec::new();
        }

        let find_output = String::from_utf8(find_output.stdout).unwrap();

        find_output
            .lines()
            .map(|line| Path::new(line))
            .map(|path| {
                let name = path.file_name().unwrap();
                let group = path.parent().unwrap().file_name().unwrap();
                RunID::new(name, group)
            })
            .collect()
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
        let log_path = run_id.path(&self.output_base_dir_path).join("logs");

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
        std::process::Command::new(std::env::var("SHELL").unwrap())
            .arg("-c")
            .arg(&format!(
                "ssh -tt {} 'exec tmux attach-session -t {run_id}'",
                self.hostname
            ))
            .exec();
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
        std::process::Command::new(std::env::var("SHELL").unwrap())
            .arg("-c")
            .arg(&format!(
                "ssh -tt {} 'exec {cmd} {log_file_path}'",
                self.hostname
            ))
            .exec();
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

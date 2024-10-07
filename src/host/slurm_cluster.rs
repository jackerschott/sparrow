use super::connection::Connection;
use super::rsync::SyncOptions;
use super::{ExperimentID, Host, RunDirectory, RunDirectoryInner};
use crate::utils::Utf8Path;
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use std::os::unix::process::CommandExt;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

pub enum QuickRun {
    Disabled,
    Enabled {
        fast_access_container_paths: Vec<PathBuf>,
    },
}

pub struct SlurmClusterHost {
    id: String,
    experiment_base_dir_path: PathBuf,
    temporary_dir_path: PathBuf,

    hostname: String,
    connection: Connection,
    quick_run_config: QuickRun,
}

impl SlurmClusterHost {
    const QUICK_RUN_TOWEL_JOB_NAME: &str = "quick-run-towel";

    pub fn new(
        id: &str,
        hostname: &str,
        experiment_base_dir_path: &Path,
        temporary_dir_path: &Path,
        quick_run_config: QuickRun,
    ) -> Self {
        let hostname = if let QuickRun::Enabled { .. } = quick_run_config {
            &format!("{hostname}-quick")
        } else {
            hostname
        };

        let connection = match Connection::new(hostname) {
            Ok(connection) => connection,
            Err(e) => {
                eprintln!("Failed to connect to host {}: {}", hostname, e);
                if let QuickRun::Enabled { .. } = quick_run_config {
                    eprintln!("Did you forget to prepare the remote?")
                }
                std::process::exit(1);
            }
        };

        return Self {
            id: id.to_owned(),
            hostname: hostname.to_owned(),
            experiment_base_dir_path: experiment_base_dir_path.to_owned(),
            temporary_dir_path: temporary_dir_path.to_owned(),
            connection,
            quick_run_config,
        };
    }
}

impl SlurmClusterHost {
    pub fn allocate_quick_run_node(&self, fast_access_container_paths: &Vec<PathBuf>) {
        let submission_script = Self::build_quick_run_towel_job_script(fast_access_container_paths);

        let partition_ids = self.get_quick_run_towel_partition_ids();
        let submission_options = Self::quick_run_towel_job_submission_options(&partition_ids);

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

        status
            .exit_ok()
            .expect("expected scancel to have a successful exit code");
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

    fn get_quick_run_towel_partition_ids(&self) -> Vec<String> {
        let sinfo_output = self
            .connection
            .command("sinfo")
            .arg("-ho %R")
            .output()
            .expect("expected sinfo to succeed");

        let partition_ids =
            String::from_utf8(sinfo_output.stdout).expect("expected sinfo output to be utf-8");
        let partition_ids = partition_ids
            .trim()
            .split("\n")
            .map(|s| s.trim().to_owned());

        return partition_ids
            .filter(|id| id.contains("gpu") && !id.contains("debug"))
            .collect();
    }

    fn submit_quick_run_towel_job(&self, script: &str, options: &Vec<String>, log_path: &Path) {
        let mut submission_command = self.connection.command("sbatch");
        let mut submission_command = submission_command
            .arg(&format!("--output={log_path}"))
            .args(options)
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
                "for var in $(env | grep '^SLURM' | cut -d= -f1); do\n",
                "    unset $var;\n",
                "done\n",
                "printf \"Going to sleep...\"\n",
                "sleep 1d",
            ),
            container_copy_loop
        )
    }

    fn quick_run_towel_job_submission_options(partition_ids: &Vec<String>) -> Vec<String> {
        vec![
            format!("--partition={}", partition_ids.join(",")),
            "--time=5:00:00".to_owned(),
            "--cpus-per-task=4".to_owned(),
            "--gpus=1".to_owned(),
            format!("--job-name={}", Self::QUICK_RUN_TOWEL_JOB_NAME),
        ]
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
    fn experiment_base_dir_path(&self) -> &Path {
        &self.experiment_base_dir_path.as_path()
    }
    fn is_local(&self) -> bool {
        false
    }
    fn is_configured_for_quick_run(&self) -> bool {
        self.hostname.ends_with("-test")
    }

    fn create_run_from_prep_dir(
        &self,
        prep_dir: TempDir,
        code_revision: Option<&str>,
    ) -> RunDirectory {
        let run_dir_path = self
            .temporary_dir_path
            .join(tmpname("experiment_code.", "", 4));
        self.connection.upload(
            prep_dir.utf8_path(),
            run_dir_path.as_path(),
            SyncOptions::default().copy_contents().delete(),
        );

        return RunDirectory {
            inner: RunDirectoryInner::Remote { run_dir_path },
            code_revision: code_revision.map(|s| s.to_owned()),
        };
    }

    fn prepare(&self) {
        match &self.quick_run_config {
            QuickRun::Enabled {
                fast_access_container_paths,
            } => self.allocate_quick_run_node(fast_access_container_paths),
            QuickRun::Disabled => {}
        }
    }
    fn is_prepared(&self) -> bool {
        match &self.quick_run_config {
            QuickRun::Enabled { .. } => self.has_allocated_quick_run_node(),
            QuickRun::Disabled => true,
        }
    }

    fn wait_for_preparation(&self) {
        match &self.quick_run_config {
            QuickRun::Enabled { .. } => {
                self.tail_quick_run_towel_submission_log(&self.quick_run_towel_log_path())
            }
            QuickRun::Disabled => {}
        }
    }

    fn clear_preparation(&self) {
        match &self.quick_run_config {
            QuickRun::Enabled { .. } => self.deallocate_quick_run_node(),
            QuickRun::Disabled => {}
        }
    }

    fn experiments(&self) -> Vec<ExperimentID> {
        if !self.experiment_base_dir_path.as_path().exists() {
            return Vec::new();
        }

        let find_output = self
            .connection
            .command("find")
            .arg(self.experiment_base_dir_path.as_str())
            .arg("-mindepth")
            .arg("1")
            .arg("-maxdepth")
            .arg("1")
            .arg("-type")
            .arg("d")
            .output()
            .expect("expected experiment find to succeed");

        if !find_output.status.success() {
            return Vec::new();
        }

        let find_output = String::from_utf8(find_output.stdout).unwrap();

        find_output
            .lines()
            .map(|line| Path::new(line))
            .map(|path| {
                let group = path.file_name().unwrap();
                let name = path.parent().unwrap().file_name().unwrap();
                ExperimentID::new(name, group)
            })
            .collect()
    }
    fn running_experiments(&self) -> Vec<ExperimentID> {
        let tmux_output = self
            .connection
            .command("tmux")
            .arg("list-sessions")
            .output()
            .expect("expected experiment find to succeed");

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
                ExperimentID::new(name, group)
            })
            .collect()
    }
    fn log_file_paths(&self, experiment_id: &ExperimentID) -> Vec<PathBuf> {
        let log_path = experiment_id
            .path(&self.experiment_base_dir_path)
            .join("logs");

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
                path.strip_prefix(&experiment_id.path(&self.experiment_base_dir_path))
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }
    fn attach(&self, experiment_id: &ExperimentID) {
        std::process::Command::new(std::env::var("SHELL").unwrap())
            .arg("-c")
            .arg(&format!(
                "ssh -tt {} 'exec tmux attach-session -t {experiment_id}'",
                self.hostname
            ))
            .exec();
    }
    fn sync(&self, experiment_id: &ExperimentID, local_base_path: &Path) {
        self.connection.download(
            &experiment_id.path(&self.experiment_base_dir_path),
            &local_base_path,
            SyncOptions::default().copy_contents().delete(),
        );
    }
    fn tail_log(&self, experiment_id: &ExperimentID, log_file_path: &Path, follow: bool) {
        let log_file_path = experiment_id
            .path(&self.experiment_base_dir_path)
            .join(log_file_path);
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

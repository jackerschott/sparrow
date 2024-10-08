pub mod connection;
pub mod local;
pub mod rsync;
pub mod slurm_cluster;

use super::utils::Utf8Path;
use crate::cfg::{HostType, LocalHostConfig, QuickRunConfig, RemoteHostConfig};
use crate::payload::{CodeSource, ConfigSource, PayloadSource};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use local::LocalHost;
use rsync::{copy_directory, SyncOptions};
use slurm_cluster::{QuickRun, SlurmClusterHost};
use tempfile::NamedTempFile;
use tempfile::TempDir;
use url::Url;

pub trait Host {
    fn id(&self) -> &str;
    fn hostname(&self) -> &str;
    fn experiment_base_dir_path(&self) -> &Path;
    fn is_local(&self) -> bool;
    fn is_configured_for_quick_run(&self) -> bool;

    fn info(&self) -> HostInfo {
        HostInfo {
            id: self.id().to_owned(),
            hostname: self.hostname().to_owned(),
            experiment_base_dir_path: self.experiment_base_dir_path().to_owned(),
            is_local: self.is_local(),
            is_configured_for_quick_run: self.is_configured_for_quick_run(),
        }
    }

    fn prepare_run_directory(
        &self,
        payload_source: &PayloadSource,
        run_script: NamedTempFile,
        review_config: bool,
    ) -> RunDirectory {
        println!("Prepare code...");
        let payload_prep_dir = prepare_code(&payload_source.code_source);

        println!("Prepare config...");
        let (config_prep_dir, config_paths) = prepare_config(&payload_source.config_source);
        if review_config {
            self::review_config(
                &config_paths,
                &config_prep_dir.utf8_path().join(
                    &payload_source.config_source.main_config_path
                ),
            );
        }

        copy_directory(
            config_prep_dir.utf8_path(),
            payload_prep_dir.utf8_path(),
            SyncOptions::default().copy_contents(),
        );

        let run_script_dest_path = payload_prep_dir.utf8_path().join("run.sh");
        std::fs::copy(&run_script, &run_script_dest_path).expect(&format!(
            "expected copy from {} to {} to work",
            run_script.utf8_path(),
            run_script_dest_path
        ));

        println!("Prepare run directory...");
        if let CodeSource::Remote { git_revision, .. } = &payload_source.code_source {
            self.create_run_from_prep_dir(payload_prep_dir, Some(git_revision.as_str()))
        } else {
            self.create_run_from_prep_dir(payload_prep_dir, None)
        }
    }

    fn create_run_from_prep_dir(
        &self,
        prep_dir: TempDir,
        code_revision: Option<&str>,
    ) -> RunDirectory;

    fn prepare_quick_run(&self, options: &HostPreparationOptions);
    #[allow(unused)]
    fn quick_run_is_prepared(&self) -> bool;
    fn wait_for_preparation(&self);
    fn clear_preparation(&self);

    fn experiments(&self) -> Vec<ExperimentID>;
    fn running_experiments(&self) -> Vec<ExperimentID>;
    fn log_file_paths(&self, experiment_id: &ExperimentID) -> Vec<PathBuf>;
    fn attach(&self, experiment_id: &ExperimentID);
    fn sync(&self, experiment_id: &ExperimentID, local_base_path: &Path);
    fn tail_log(&self, experiment_id: &ExperimentID, log_file_path: &Path, follow: bool);
}

pub enum HostPreparationOptions {
    SlurmCluster {
        time: String,
        cpu_count: u16,
        gpu_count: u16,
        fast_access_container_paths: Vec<PathBuf>,
    },
    Local {},
}

impl HostPreparationOptions {
    pub fn build(
        host_type: &HostType,
        time: Option<&str>,
        cpu_count: Option<u16>,
        gpu_count: Option<u16>,
        quick_run_config: &QuickRunConfig,
    ) -> Self {
        match host_type {
            HostType::Local => HostPreparationOptions::Local {},
            HostType::Remote => HostPreparationOptions::SlurmCluster {
                time: time.unwrap_or(&quick_run_config.time).to_owned(),
                cpu_count: cpu_count.unwrap_or(quick_run_config.cpu_count),
                gpu_count: gpu_count.unwrap_or(quick_run_config.gpu_count),
                fast_access_container_paths: quick_run_config
                    .fast_access_container_requests
                    .clone(),
            },
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct ExperimentID {
    pub name: String,
    pub group: String,
}

impl ExperimentID {
    pub fn new<S: AsRef<str>>(name: S, group: S) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            group: group.as_ref().to_owned(),
        }
    }

    pub fn path<P: Into<PathBuf>>(&self, base_path: P) -> PathBuf {
        base_path
            .into()
            .join(self.group.clone())
            .join(self.name.clone())
    }
}

impl std::fmt::Display for ExperimentID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.group, self.name)
    }
}

#[derive(serde::Serialize)]
pub struct HostInfo {
    pub id: String,
    pub hostname: String,
    pub experiment_base_dir_path: PathBuf,
    pub is_local: bool,
    pub is_configured_for_quick_run: bool,
}

pub enum RunDirectoryInner {
    Remote { run_dir_path: PathBuf },
    Local { run_dir: TempDir },
}
pub struct RunDirectory {
    inner: RunDirectoryInner,

    #[allow(unused)]
    code_revision: Option<String>,
}

impl RunDirectory {
    pub fn path(&self) -> &Path {
        match &self.inner {
            RunDirectoryInner::Remote { run_dir_path } => run_dir_path.as_path(),
            RunDirectoryInner::Local { run_dir } => run_dir.utf8_path(),
        }
    }

    #[allow(unused)]
    pub fn code_revision(&self) -> Option<&str> {
        self.code_revision.as_deref()
    }
}

pub fn build_host(
    host_type: HostType,
    local_config: &LocalHostConfig,
    remote_config: &RemoteHostConfig,
    configure_for_quick_run: bool,
) -> Result<Box<dyn Host>, String> {
    if host_type == HostType::Local && configure_for_quick_run {
        return Err("Cannot use --enforce-quick with the local host".to_owned());
    }

    match host_type {
        HostType::Local => Ok(Box::new(LocalHost::new(
            local_config.experiment_base_dir.as_path(),
        ))),
        HostType::Remote => {
            let quick_run_config = if !configure_for_quick_run {
                QuickRun::Disabled
            } else {
                QuickRun::Enabled
            };

            Ok(Box::new(SlurmClusterHost::new(
                remote_config.id.as_str(),
                remote_config.hostname.as_str(),
                remote_config.experiment_base_dir.as_path(),
                remote_config.temporary_dir.as_path(),
                quick_run_config,
            )))
        }
    }
}

fn prepare_code(code_source: &CodeSource) -> TempDir {
    let prep_dir = TempDir::new().expect("failed to create temporary directory");
    match code_source {
        CodeSource::Local {
            path,
            copy_excludes,
        } => {
            copy_directory(
                path.as_path(),
                prep_dir.utf8_path(),
                SyncOptions::default()
                    .copy_contents()
                    .exclude(&copy_excludes),
            );
        }
        CodeSource::Remote { url, git_revision } => {
            unpack_revision(
                &url,
                git_revision.as_str(),
                prep_dir.utf8_path(),
                Path::new(&format!(
                    "{}/.ssh/id_ed25519",
                    std::env::var("HOME").unwrap()
                )),
            );
        }
    }

    return prep_dir;
}

fn prepare_config(config_source: &ConfigSource) -> (TempDir, Vec<PathBuf>) {
    let prep_dir = TempDir::new().expect("failed to create temporary directory");

    let mut new_config_paths: Vec<PathBuf> = Vec::new();
    for config_path in &config_source.config_paths {
        let config_source_path = config_source.base_path.join(config_path.as_str());
        let config_dest_path = prep_dir.utf8_path().join(config_path.as_str());

        std::fs::create_dir_all(config_dest_path.as_path())
            .expect(&format!("expected creation of {config_dest_path} to work"));

        copy_directory(
            config_source_path.as_path(),
            config_dest_path.as_path(),
            SyncOptions::default()
                .copy_contents()
                .exclude(&config_source.copy_excludes),
        );

        new_config_paths.push(config_dest_path);
    }

    return (prep_dir, new_config_paths);
}

fn review_config(config_paths: &Vec<PathBuf>, main_config_path: &Path) {
    let editor_name = std::env::var("EDITOR").expect("EDITOR variable should be set");
    let mut cmd = std::process::Command::new(editor_name);

    cmd.arg(main_config_path.as_str());
    for path in config_paths.iter() {
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.expect("expected config dir walking to work");
            if entry.path() == main_config_path {
                continue;
            }

            cmd.arg(entry.path());
        }
    }

    cmd.status().expect("expected {cmd} to run successfully");
}

fn unpack_revision(url: &Url, git_revision: &str, destination_path: &Path, ssh_key_path: &Path) {
    // build lambda for fetch options
    let get_fetch_options = || {
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(|_url, _username_from_url, _allowed_types| {
            git2::Cred::ssh_key("git", None, ssh_key_path.as_std_path(), None)
        });

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);
        return fetch_options;
    };

    let repo = git2::build::RepoBuilder::new()
        .fetch_options(get_fetch_options())
        .clone(url.as_str(), destination_path.as_std_path())
        .expect(&format!("cloning {url} to {destination_path} should work"));

    let revision = repo.revparse(git_revision).expect(&format!(
        "revision {git_revision} should be valid\nDid you push it?"
    ));
    let treeish = revision.from().expect(&format!(
        "expected {git_revision} to be a single revision \
        and single revisions to have a from"
    ));

    repo.checkout_tree(&treeish, None)
        .expect(&format!("expected checkout to {git_revision} to work"));

    let mut submodules = repo
        .submodules()
        .expect("expected submodules to be accessible");

    let mut submodule_update_opts = git2::SubmoduleUpdateOptions::new();
    submodule_update_opts.fetch(get_fetch_options());
    submodules.iter_mut().for_each(|submodule| {
        submodule
            .update(true, Some(&mut submodule_update_opts))
            .expect(&format!("expected update of submodule to work"));
    });
}

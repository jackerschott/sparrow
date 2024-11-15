pub mod connection;
pub mod local;
pub mod rsync;
pub mod slurm_cluster;

use std::collections::HashMap;

use super::utils::Utf8Path;
use crate::cfg::{LocalHostConfig, QuickRunConfig, RemoteHostConfig};
use crate::payload::{AuxiliaryMapping, CodeMapping, CodeSource, ConfigSource};
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
    fn output_base_dir_path(&self) -> &Path;
    fn is_local(&self) -> bool;
    fn is_configured_for_quick_run(&self) -> bool;

    fn info(&self) -> HostInfo {
        HostInfo {
            id: self.id().to_owned(),
            hostname: self.hostname().to_owned(),
            run_output_base_dir_path: self.output_base_dir_path().to_owned(),
            is_local: self.is_local(),
            is_configured_for_quick_run: self.is_configured_for_quick_run(),
        }
    }

    fn prepare_run_directory(
        &self,
        code_mappings: &Vec<CodeMapping>,
        auxiliary_mappings: &Vec<AuxiliaryMapping>,
        run_script: NamedTempFile,
    ) -> RunDirectory {
        let payload_prep_dir = TempDir::new().expect("failed to create temporary directory");

        for code_mapping in code_mappings {
            prepare_code(code_mapping, payload_prep_dir.utf8_path());
        }

        for auxiliary_mapping in auxiliary_mappings {
            copy_directory(
                &auxiliary_mapping.source_path,
                &payload_prep_dir
                    .utf8_path()
                    .join(&auxiliary_mapping.target_path),
                SyncOptions::default()
                    .copy_contents()
                    .exclude(&auxiliary_mapping.copy_excludes),
            );
        }

        let run_script_dest_path = payload_prep_dir.utf8_path().join("run.sh");
        std::fs::copy(&run_script, &run_script_dest_path).expect(&format!(
            "expected copy from {} to {} to work",
            run_script.utf8_path(),
            run_script_dest_path
        ));

        return self.upload_run_dir(payload_prep_dir);
    }

    fn upload_run_dir(&self, prep_dir_path: TempDir) -> RunDirectory;

    fn prepare_config_directory(
        &self,
        config_mapping: &ConfigSource,
        run_id: &RunID,
        review: bool,
    ) {
        let review_dir = TempDir::new().expect("expected temporary directory creation to work");

        copy_directory(
            &config_mapping.dir_path,
            &review_dir.utf8_path(),
            SyncOptions::default().copy_contents(),
        );

        if review {
            let entry_path = review_dir.utf8_path().join(&config_mapping.entrypoint_path);
            review_config(review_dir.utf8_path(), &entry_path);
        }

        self.create_dir_all(&self.config_dir_destination_path(run_id));

        self.put(
            review_dir.utf8_path(),
            &self.config_dir_destination_path(run_id),
            SyncOptions::default().copy_contents(),
        )
    }

    fn config_dir_destination_path(&self, run_id: &RunID) -> PathBuf {
        run_id
            .path(self.output_base_dir_path())
            .join("reproduce_info/config")
    }

    fn put(&self, local_path: &Path, host_path: &Path, options: SyncOptions);
    #[allow(unused)]
    fn create_dir(&self, path: &Path);
    fn create_dir_all(&self, path: &Path);

    fn prepare_quick_run(&self, options: &QuickRunPrepOptions);
    #[allow(unused)]
    fn quick_run_is_prepared(&self) -> bool;
    fn wait_for_preparation(&self);
    fn clear_preparation(&self);

    fn runs(&self) -> Vec<RunID>;
    fn running_runs(&self) -> Vec<RunID>;
    fn log_file_paths(&self, run_id: &RunID) -> Vec<PathBuf>;
    fn attach(&self, run_id: &RunID);
    fn sync(
        &self,
        run_id: &RunID,
        local_base_path: &Path,
        options: &RunOutputSyncOptions,
    ) -> Result<(), String>;
    fn tail_log(&self, run_id: &RunID, log_file_path: &Path, follow: bool);
}

pub enum RunDirectory {
    Local(TempDir),
    Remote(PathBuf),
}

impl RunDirectory {
    pub fn path(&self) -> &Path {
        match self {
            RunDirectory::Local(dir) => dir.utf8_path(),
            RunDirectory::Remote(path) => path,
        }
    }
}

pub enum QuickRunPrepOptions {
    SlurmCluster {
        time: String,
        cpu_count: u16,
        gpu_count: u16,
        fast_access_container_paths: Vec<PathBuf>,
    },
}

impl QuickRunPrepOptions {
    pub fn build(
        time: Option<&str>,
        cpu_count: Option<u16>,
        gpu_count: Option<u16>,
        quick_run_config: &QuickRunConfig,
    ) -> Self {
        QuickRunPrepOptions::SlurmCluster {
            time: time.unwrap_or(&quick_run_config.time).to_owned(),
            cpu_count: cpu_count.unwrap_or(quick_run_config.cpu_count),
            gpu_count: gpu_count.unwrap_or(quick_run_config.gpu_count),
            fast_access_container_paths: quick_run_config.fast_access_container_requests.clone(),
        }
    }
}

pub struct RunOutputSyncOptions {
    pub excludes: Vec<String>,
    pub ignore_from_remote_marker: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct RunID {
    pub name: String,
    pub group: String,
}

impl RunID {
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

impl std::fmt::Display for RunID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.group, self.name)
    }
}

#[derive(serde::Serialize)]
pub struct HostInfo {
    pub id: String,
    pub hostname: String,
    pub run_output_base_dir_path: PathBuf,
    pub is_local: bool,
    pub is_configured_for_quick_run: bool,
}

pub fn build_host(
    host_id: &str,
    local_config: &LocalHostConfig,
    remote_configs: &HashMap<String, RemoteHostConfig>,
    configure_for_quick_run: bool,
) -> Result<Box<dyn Host>, String> {
    if host_id == "local" && configure_for_quick_run {
        return Err("Cannot use --enforce-quick with the local host".to_owned());
    }

    if host_id == "local" {
        Ok(Box::new(LocalHost::new(
            local_config.run_output_base_dir.as_path(),
        )))
    } else if remote_configs.contains_key(host_id) {
        let quick_run_config = if !configure_for_quick_run {
            QuickRun::Disabled
        } else {
            QuickRun::Enabled
        };

        Ok(Box::new(SlurmClusterHost::new(
            &host_id,
            remote_configs[host_id].hostname.as_str(),
            remote_configs[host_id].run_output_base_dir.as_path(),
            remote_configs[host_id].temporary_dir.as_path(),
            quick_run_config,
        )))
    } else {
        Err(format!("Unknown host id: {}", host_id))
    }
}

fn prepare_code(code_mapping: &CodeMapping, prep_dir: &Path) {
    assert!(code_mapping.target_path.is_relative());

    match &code_mapping.source {
        CodeSource::Local {
            path,
            copy_excludes,
        } => {
            copy_directory(
                path.as_path(),
                &prep_dir.join(code_mapping.target_path.as_path()),
                SyncOptions::default()
                    .copy_contents()
                    .exclude(&copy_excludes),
            );
        }
        CodeSource::Remote { url, git_revision } => {
            unpack_revision(
                &url,
                git_revision.as_str(),
                &prep_dir.join(code_mapping.target_path.as_path()),
                Path::new(&format!(
                    "{}/.ssh/id_ed25519",
                    std::env::var("HOME").unwrap()
                )),
            );
        }
    }
}

fn review_config(dir_path: &Path, entrypoint_path: &Path) {
    let terminal_name = std::env::var("TERMINAL").expect("expected TERMINAL variable to be set");
    let editor_name = std::env::var("EDITOR").expect("expected EDITOR variable to be set");
    let mut cmd = std::process::Command::new(terminal_name);

    cmd.arg("-e").arg(editor_name).arg(entrypoint_path.as_str());
    for entry in walkdir::WalkDir::new(dir_path) {
        let entry = entry.expect("expected config dir walking to work");
        if entry.path() == entrypoint_path {
            continue;
        }

        cmd.arg(entry.path());
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

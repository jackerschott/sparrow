pub mod connection;
pub mod local;
pub mod rsync;
pub mod slurm_cluster;

use super::utils::Utf8Path;
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use enum_dispatch::enum_dispatch;
use local::LocalHost;
use rsync::{copy_directory, rsync, SyncOptions, SyncPayload};
use slurm_cluster::SlurmClusterHost;
use tempfile::TempDir;
use url::Url;

#[derive(Clone)]
pub enum CodeSource {
    Remote {
        url: Url,
        git_revision: String,
    },
    Local {
        path: PathBuf,
        copy_excludes: Vec<String>,
    },
}
#[derive(Clone)]
pub struct ConfigSource {
    pub base_path: PathBuf,
    pub config_paths: Vec<String>,
    pub copy_excludes: Vec<String>,
}

#[derive(Clone)]
pub struct PayloadSource {
    pub code_source: CodeSource,
    pub config_source: ConfigSource,
}

pub enum RunDirectory {
    Remote { run_dir_path: PathBuf },
    Local { run_dir: TempDir },
}

pub trait Host {
    fn id(&self) -> &str;
    fn experiment_base_dir_path(&self) -> &Path;
    fn is_local(&self) -> bool;

    fn prepare_run_directory(&self, payload_source: &PayloadSource) -> RunDirectory {
        let payload_prep_dir = prepare_code(&payload_source.code_source);

        let (config_prep_dir, config_paths) = prepare_config(&payload_source.config_source);
        //let config_prep_dir = review_config(config_prep_dir, &config_paths);

        copy_directory(
            config_prep_dir.utf8_path(),
            payload_prep_dir.utf8_path(),
            SyncOptions::default().copy_contents().delete(),
        );

        return self.create_run_from_prep_dir(config_prep_dir);
    }

    fn create_run_from_prep_dir(&self, prep_dir: TempDir) -> RunDirectory;
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
            SyncOptions::default().copy_contents(),
        );

        new_config_paths.push(config_dest_path);
    }

    return (prep_dir, new_config_paths);
}

fn review_config(prep_dir: TempDir, config_paths: &Vec<PathBuf>) -> TempDir {
    let editor_name = std::env::var("EDITOR").expect("EDITOR variable should be set");
    let mut cmd = std::process::Command::new(editor_name);

    for config_path in config_paths {
        cmd.arg(config_path.as_str());
    }

    cmd.status().expect("expected {cmd} to run successfully");

    return prep_dir;
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

    let revision = repo
        .revparse(git_revision)
        .expect(&format!("revision {git_revision} should be valid"));
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

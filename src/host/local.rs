use super::rsync::{copy_directory, SyncOptions};
use super::{ExperimentID, ExperimentSyncOptions, Host, QuickRunPrepOptions, RunDirectory};
use crate::utils::{AsUtf8Path, Utf8Str};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

pub struct LocalHost {
    experiment_base_dir_path: PathBuf,
}

impl LocalHost {
    pub fn new(experiment_base_dir_path: &Path) -> Self {
        return Self {
            experiment_base_dir_path: PathBuf::from(experiment_base_dir_path),
        };
    }
}

impl Host for LocalHost {
    fn id(&self) -> &str {
        "local"
    }
    fn hostname(&self) -> &str {
        "localhost"
    }
    fn experiment_base_dir_path(&self) -> &Path {
        &self.experiment_base_dir_path.as_path()
    }

    fn is_local(&self) -> bool {
        true
    }

    fn is_configured_for_quick_run(&self) -> bool {
        true
    }

    fn run_dir(&self, prep_dir: tempfile::TempDir) -> RunDirectory {
        return RunDirectory::Local(prep_dir);
    }

    fn put(&self, local_path: &Path, host_path: &Path, options: SyncOptions) {
        if local_path != host_path {
            copy_directory(local_path, host_path, options);
        }
    }

    fn prepare_quick_run(&self, _options: &QuickRunPrepOptions) {}
    fn quick_run_is_prepared(&self) -> bool {
        true
    }
    fn wait_for_preparation(&self) {}
    fn clear_preparation(&self) {}

    fn experiments(&self) -> Vec<ExperimentID> {
        if !self.experiment_base_dir_path.as_path().exists() {
            return Vec::new();
        }

        let mut ids = Vec::new();
        for group_dir in std::fs::read_dir(self.experiment_base_dir_path.as_path())
            .expect("expected read of experiment base dir to succeed")
        {
            let group_dir = group_dir.expect("expected read of experiment base dir to succeed");
            for name_dir in std::fs::read_dir(group_dir.path())
                .expect("expected read of experiment group dir to succeed")
            {
                let name_dir = name_dir.expect("expected read of experiment group dir to succeed");

                assert!(group_dir
                    .file_type()
                    .expect("expected file_type to be accessible")
                    .is_dir());
                assert!(name_dir
                    .file_type()
                    .expect("expected file_type to be accessible")
                    .is_dir());

                ids.push(ExperimentID::new(
                    name_dir.file_name().utf8_str(),
                    group_dir.file_name().utf8_str(),
                ));
            }
        }

        return ids;
    }
    fn running_experiments(&self) -> Vec<ExperimentID> {
        unimplemented!();
    }
    fn log_file_paths(&self, experiment_id: &ExperimentID) -> Vec<PathBuf> {
        let log_path = experiment_id
            .path(&self.experiment_base_dir_path)
            .join("logs");
        walkdir::WalkDir::new(log_path)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "log")
                    .unwrap_or(false)
            })
            .map(|entry| entry.path().as_utf8().to_owned())
            .collect()
    }
    fn attach(&self, _experiment_id: &ExperimentID) {
        unimplemented!();
    }
    fn sync(
        &self,
        _experiment_id: &ExperimentID,
        _local_base_path: &Path,
        _options: &ExperimentSyncOptions,
    ) {
    }
    fn tail_log(&self, _experiment_id: &ExperimentID, _log_file_path: &Path, _follow: bool) {
        unimplemented!();
    }
}

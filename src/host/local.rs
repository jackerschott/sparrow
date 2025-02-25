use super::rsync::{copy_directory, SyncOptions};
use super::{Host, QuickRunPrepOptions, RunDirectory, RunID, RunOutputSyncOptions};
use crate::utils::{AsUtf8Path, Utf8Str};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

pub struct LocalHost {
    output_base_dir_path: PathBuf,
}

impl LocalHost {
    pub fn new(output_base_dir_path: &Path) -> Self {
        return Self {
            output_base_dir_path: PathBuf::from(output_base_dir_path),
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
    fn output_base_dir_path(&self) -> &Path {
        &self.output_base_dir_path.as_path()
    }

    fn is_local(&self) -> bool {
        true
    }

    fn is_configured_for_quick_run(&self) -> bool {
        true
    }

    fn upload_run_dir(&self, prep_dir: tempfile::TempDir) -> RunDirectory {
        return RunDirectory::Local(prep_dir);
    }

    fn put(&self, local_path: &Path, host_path: &Path, options: SyncOptions) {
        if local_path != host_path {
            copy_directory(local_path, host_path, options);
        }
    }

    fn create_dir(&self, path: &Path) {
        std::fs::create_dir(path).expect(&format!("expected creation of {path} to work"));
    }

    fn create_dir_all(&self, path: &Path) {
        std::fs::create_dir_all(path).expect(&format!("expected creation of {path} to work"));
    }

    fn prepare_quick_run(&self, _options: &QuickRunPrepOptions) {}
    fn quick_run_is_prepared(&self) -> bool {
        true
    }
    fn wait_for_preparation(&self) {}
    fn clear_preparation(&self) {}

    fn runs(&self) -> Vec<RunID> {
        if !self.output_base_dir_path.as_path().exists() {
            return Vec::new();
        }

        let mut ids = Vec::new();
        for group_dir in std::fs::read_dir(self.output_base_dir_path.as_path())
            .expect("expected read of run output base dir to succeed")
        {
            let group_dir = group_dir.expect("expected read of run output base dir to succeed");
            for name_dir in std::fs::read_dir(group_dir.path())
                .expect("expected read of run output group dir to succeed")
            {
                let name_dir = name_dir.expect("expected read of run output group dir to succeed");

                assert!(group_dir
                    .file_type()
                    .expect("expected file_type to be accessible")
                    .is_dir());
                assert!(name_dir
                    .file_type()
                    .expect("expected file_type to be accessible")
                    .is_dir());

                ids.push(RunID::new(
                    name_dir.file_name().utf8_str(),
                    group_dir.file_name().utf8_str(),
                ));
            }
        }

        return ids;
    }
    fn running_runs(&self) -> Vec<RunID> {
        unimplemented!();
    }
    fn log_file_paths(&self, run_id: &RunID) -> Vec<PathBuf> {
        let log_path = run_id.path(&self.output_base_dir_path).join("logs");
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
    fn attach(&self, _run_id: &RunID) {
        unimplemented!();
    }
    fn sync(
        &self,
        _run_id: &RunID,
        _local_base_path: &Path,
        _options: &RunOutputSyncOptions,
    ) -> Result<(), String> {
        Ok(())
    }
    fn tail_log(&self, _run_id: &RunID, _log_file_path: &Path, _follow: bool) {
        unimplemented!();
    }
}

pub fn show_result(run_id: &RunID, base_path: &Path, path: &Path) {
    let result_path = run_id.path(base_path).join(path);
    open::that_detached(&result_path)
        .expect(&format!("failed to open `{result_path}' with the system default application"));
}

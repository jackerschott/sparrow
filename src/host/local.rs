use super::rsync::{copy_directory, SyncOptions};
use super::{Host, QuickRunPrepOptions, RunDirectory, RunID, RunOutputSyncOptions};
use crate::utils::{AsUtf8Path, Utf8Str};
use anyhow::{Context, Result};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

pub struct LocalHost {
    output_base_dir_path: PathBuf,
    script_run_command_template: String,
}

impl LocalHost {
    pub fn new(output_base_dir_path: &Path, script_run_command_template: String) -> Self {
        return Self {
            output_base_dir_path: PathBuf::from(output_base_dir_path),
            script_run_command_template,
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
    fn script_run_command(&self, script_path: &str) -> String {
        return self.script_run_command_template.replace("{}", script_path)
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
    fn download_config_dir(&self, _local: &LocalHost, run_id: &RunID) -> Result<PathBuf> {
        Ok(self.config_dir_destination_path(run_id))
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

    fn prepare_quick_run(&self, _options: &QuickRunPrepOptions) -> Result<()> { Ok(()) }
    fn quick_run_is_prepared(&self) -> Result<bool> {
        Ok(true)
    }
    fn clear_preparation(&self) {}

    fn runs(&self) -> Result<Vec<RunID>> {
        if !self.output_base_dir_path.as_path().exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for group_dir in std::fs::read_dir(self.output_base_dir_path.as_path())
            .context(format!("failed to read {}", self.output_base_dir_path))?
        {
            let group_dir = group_dir.context(format!("failed to read {}", self.output_base_dir_path))?;
            for name_dir in std::fs::read_dir(group_dir.path())
                .expect("expected read of run output group dir to succeed")
            {
                let name_dir = name_dir.context(format!("failed to read {}", self.output_base_dir_path))?;

                assert!(group_dir
                    .file_type()
                    .context(format!("failed to obtain file type for {}", group_dir.path().as_utf8()))?
                    .is_dir());
                assert!(name_dir
                    .file_type()
                    .context(format!("failed to obtain file type for {}", name_dir.path().as_utf8()))?
                    .is_dir());

                ids.push(RunID::new(
                    name_dir.file_name().utf8_str(),
                    group_dir.file_name().utf8_str(),
                ));
            }
        }

        Ok(ids)
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

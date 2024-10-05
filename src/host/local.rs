use super::{Host, RunDirectory};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use tempfile::TempDir;

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
        return "local";
    }

    fn experiment_base_dir_path(&self) -> &Path {
        return &self.experiment_base_dir_path.as_path();
    }

    fn is_local(&self) -> bool {
        return true;
    }

    fn create_run_from_prep_dir(&self, prep_dir: TempDir) -> RunDirectory {
        return RunDirectory::Local { run_dir: prep_dir };
    }
}

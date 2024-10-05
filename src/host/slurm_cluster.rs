use super::connection::Connection;
use super::rsync::SyncOptions;
use super::Utf8Path;
use super::{Host, RunDirectory};
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use std::str::FromStr;
use tempfile::TempDir;
use futures::executor::block_on;

pub struct SlurmClusterHost {
    id: String,
    experiment_base_dir_path: PathBuf,
    temporary_dir_path: PathBuf,

    connection: Connection,
}

impl SlurmClusterHost {
    pub fn new(
        id: &str,
        hostname: &str,
        experiment_base_dir_path: &Path,
        temporary_dir_path: &Path,
    ) -> Self {
        return Self {
            id: id.to_owned(),
            experiment_base_dir_path: experiment_base_dir_path.to_owned(),
            temporary_dir_path: temporary_dir_path.to_owned(),
            connection: block_on(Connection::new(hostname)),
        };
    }
}

impl Host for SlurmClusterHost {
    fn id(&self) -> &str {
        return &self.id;
    }

    fn experiment_base_dir_path(&self) -> &Path {
        return &self.experiment_base_dir_path.as_path();
    }

    fn is_local(&self) -> bool {
        return false;
    }

    fn create_run_from_prep_dir(&self, prep_dir: TempDir) -> RunDirectory {
        let run_dir_path = self
            .temporary_dir_path
            .join(tmpname("experiment_code.", "", 4));
        self.connection.upload(
            prep_dir.utf8_path(),
            run_dir_path.as_path(),
            SyncOptions::default().copy_contents().delete(),
        );

        return RunDirectory::Remote { run_dir_path };
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

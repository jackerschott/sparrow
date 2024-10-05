use camino::Utf8Path as Path;
use tempfile::TempDir;

pub trait Utf8Path {
    fn utf8_path(&self) -> &Path;
}

impl Utf8Path for TempDir {
    fn utf8_path(&self) -> &Path {
        return Path::from_path(self.path())
            .expect("temporary directory path is not a valid utf8 string");
    }
}

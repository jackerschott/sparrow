use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::vec::Vec;

pub enum SyncPayload<'a> {
    LocalToRemote {
        control_path: &'a Path,
        sources: &'a Vec<&'a Path>,
        destination: &'a Path,
    },
    RemoteToLocal {
        control_path: &'a Path,
        source: &'a Path,
        destination: &'a Path,
    },
    LocalToLocal {
        sources: &'a Vec<&'a Path>,
        destination: &'a Path,
    },
}

#[derive(Debug)]
pub struct SyncOptions {
    quiet: bool,
    delete: bool,
    excludes: Vec<String>,
    infos: Vec<String>,
    copy_contents: bool,
    create_missing_path_components: bool,
}
impl SyncOptions {
    pub fn default() -> SyncOptions {
        SyncOptions {
            quiet: false,
            delete: false,
            excludes: Vec::new(),
            infos: Vec::new(),
            copy_contents: false,
            create_missing_path_components: false,
        }
    }

    #[allow(unused)]
    pub fn quiet(mut self) -> SyncOptions {
        self.quiet = true;
        self
    }

    pub fn delete(mut self) -> SyncOptions {
        self.delete = true;
        self
    }

    pub fn exclude(mut self, excludes: &Vec<String>) -> SyncOptions {
        self.excludes.extend(excludes.clone());
        self
    }

    #[allow(unused)]
    pub fn info(mut self, infos: &Vec<&str>) -> SyncOptions {
        self.infos.extend(
            infos
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
                .clone(),
        );
        self
    }

    pub fn copy_contents(mut self) -> SyncOptions {
        self.copy_contents = true;
        self
    }

    pub fn create_missing_path_components(mut self) -> SyncOptions {
        self.create_missing_path_components = true;
        self
    }
}

fn ensure_trailing_slash(path: &Path) -> PathBuf {
    return PathBuf::from_str((path.as_str().to_owned() + "/").as_ref()).unwrap();
}

fn ensure_trimmed_trailing_slash(path: &Path) -> &Path {
    return Path::new(path.as_str().trim_end_matches("/"));
}

pub fn rsync<'a>(payload: SyncPayload<'a>, options: SyncOptions) -> std::io::Result<()> {
    let mut cmd = Command::new("rsync");

    cmd.args(["--recursive", "--checksum", "--links"]);

    if options.quiet {
        cmd.arg("--quiet");
    }

    if options.delete {
        cmd.arg("--delete");
    }

    if options.create_missing_path_components {
        cmd.arg("--mkpath");
    }

    if options.infos.len() > 0 {
        let infos = options.infos.join(",");
        cmd.arg(format!("--info={infos}"));
    }

    if options.excludes.len() > 0 {
        for exclude in &options.excludes {
            cmd.arg(format!("--exclude={exclude}"));
        }
    }

    let ensure_correct_source = move |source| {
        if options.copy_contents {
            ensure_trailing_slash(source)
        } else {
            ensure_trimmed_trailing_slash(source).to_owned()
        }
    };

    match payload {
        SyncPayload::LocalToRemote {
            control_path,
            sources,
            destination,
        } => {
            cmd.arg(format!("--rsh=ssh -S {control_path}").as_str());

            sources.into_iter().for_each(|source| {
                cmd.arg(ensure_correct_source(source));
            });

            cmd.arg(format!("none:{destination}"));
        }
        SyncPayload::RemoteToLocal {
            control_path,
            source,
            destination,
        } => {
            cmd.arg(format!("--rsh=ssh -S {control_path}").as_str());

            cmd.arg(format!("none:{}", ensure_correct_source(source)));
            cmd.arg(destination);
        }
        SyncPayload::LocalToLocal {
            sources,
            destination,
        } => {
            for source in sources {
                cmd.arg(ensure_correct_source(source));
            }
            cmd.arg(destination);
        }
    }

    cmd.status()?;

    Ok(())
}

pub fn copy_directory(source: &Path, destination: &Path, options: SyncOptions) {
    rsync(
        SyncPayload::LocalToLocal {
            sources: &vec![source],
            destination,
        },
        options,
    )
    .expect("rsync should not fail");
}

use crate::cfg::CodeSourceConfig;
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
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
    pub entrypoint_path: PathBuf,
    pub dir_path: PathBuf,
    pub copy_excludes: Vec<String>,
}

#[derive(Clone)]
pub struct PayloadSource {
    pub code_source: CodeSource,
    pub config_source: ConfigSource,
}

#[derive(serde::Serialize)]
pub struct PayloadInfo {
    code_revision: Option<String>,
    config_dir: PathBuf,
}

impl PayloadInfo {
    pub fn new(source: &PayloadSource, config_dir_destination_path: &Path) -> PayloadInfo {
        PayloadInfo {
            code_revision: match &source.code_source {
                CodeSource::Remote { git_revision, .. } => Some(git_revision.clone()),
                _ => None,
            },
            config_dir: config_dir_destination_path.to_owned(),
        }
    }
}

pub fn build_payload_source(
    code_source_config: &CodeSourceConfig,
    config_dir_path: Option<&Path>,
    config_entry_path: Option<&Path>,
    revision: Option<&str>,
) -> PayloadSource {
    let (config_dir_path, config_entry_path) = match (config_dir_path, config_entry_path) {
        (Some(config_dir_path), Some(config_entry_path)) => (config_dir_path, config_entry_path),
        (None, Some(config_entry_path)) => (
            config_entry_path
                .parent()
                .expect("expected config path to have a parent"),
            config_entry_path,
        ),
        (None, None) => (
            code_source_config.config.dir.as_path(),
            code_source_config.config.entrypoint.as_path(),
        ),
        _ => unreachable!(
            "expected config_dir_path to only be supplied together with config_entry_path"
        ),
    };
    assert!(config_dir_path.is_relative());
    assert!(config_dir_path != ".");
    assert!(config_entry_path.is_relative());
    assert!(config_entry_path.starts_with(config_dir_path));

    let config_source = ConfigSource {
        base_path: code_source_config.local.path.clone(),
        entrypoint_path: config_entry_path.to_owned(),
        dir_path: config_dir_path.to_owned(),
        copy_excludes: code_source_config.local.excludes.clone(),
    };

    if let Some(revision) = revision {
        PayloadSource {
            code_source: CodeSource::Remote {
                url: code_source_config.remote.url.clone(),
                git_revision: revision.to_owned(),
            },
            config_source,
        }
    } else {
        PayloadSource {
            code_source: CodeSource::Local {
                path: code_source_config.local.path.clone(),
                copy_excludes: code_source_config.local.excludes.clone(),
            },
            config_source,
        }
    }
}

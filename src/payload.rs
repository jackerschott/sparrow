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
    config_dir_override_path: Option<&Path>,
    revision: Option<&str>,
) -> PayloadSource {
    assert!(code_source_config.config.entrypoint.is_relative());
    assert!(code_source_config.config.dir.is_relative() && code_source_config.config.dir != ".");

    let config_dir_override_path = config_dir_override_path.map(|x| {
        x.is_relative()
            .then_some(code_source_config.local.path.join(x))
            .unwrap_or(x.to_owned())
    });

    let config_dir_path = config_dir_override_path.unwrap_or(
        code_source_config
            .local
            .path
            .join(code_source_config.config.dir.clone()),
    );

    let config_source = ConfigSource {
        entrypoint_path: code_source_config.config.entrypoint.to_owned(),
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

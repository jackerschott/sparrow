use crate::cfg::CodeSourceConfig;
use camino::Utf8PathBuf as PathBuf;
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
    pub main_config_path: PathBuf,
    pub config_paths: Vec<PathBuf>,
    pub copy_excludes: Vec<String>,
}

#[derive(Clone)]
pub struct PayloadSource {
    pub code_source: CodeSource,
    pub config_source: ConfigSource,
}

#[derive(serde::Serialize)]
pub struct PayloadSourceInfo {
    code_revision: Option<String>,
}

impl PayloadSource {
    pub fn info(&self) -> PayloadSourceInfo {
        PayloadSourceInfo {
            code_revision: match &self.code_source {
                CodeSource::Remote { git_revision, .. } => Some(git_revision.clone()),
                _ => None,
            },
        }
    }
}

pub fn build_payload_source(
    code_source_config: &CodeSourceConfig,
    revision: Option<&str>,
) -> PayloadSource {
    if let Some(revision) = revision {
        PayloadSource {
            code_source: CodeSource::Remote {
                url: code_source_config.remote.url.clone(),
                git_revision: revision.to_owned(),
            },
            config_source: ConfigSource {
                base_path: code_source_config.local.path.clone(),
                main_config_path: code_source_config.config.main.clone(),
                config_paths: code_source_config.config.paths.clone(),
                copy_excludes: code_source_config.local.excludes.clone(),
            },
        }
    } else {
        PayloadSource {
            code_source: CodeSource::Local {
                path: code_source_config.local.path.clone(),
                copy_excludes: code_source_config.local.excludes.clone(),
            },
            config_source: ConfigSource {
                base_path: code_source_config.local.path.clone(),
                main_config_path: code_source_config.config.main.clone(),
                config_paths: code_source_config.config.paths.clone(),
                copy_excludes: code_source_config.local.excludes.clone(),
            },
        }
    }
}

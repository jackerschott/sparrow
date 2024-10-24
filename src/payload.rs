use crate::cfg::PayloadMappingConfig;
use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use std::collections::HashMap;
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
pub struct CodeMapping {
    pub id: String,
    pub source: CodeSource,
    pub target: PathBuf,
}

#[derive(Clone)]
pub struct ConfigSource {
    pub entrypoint_path: PathBuf,
    pub dir_path: PathBuf,
}

#[derive(Clone)]
pub struct PayloadMapping {
    pub code_mappings: Vec<CodeMapping>,
    pub config_source: ConfigSource,
}

#[derive(serde::Serialize)]
pub struct PayloadInfo {
    code_revisions: HashMap<String, String>,
    config_dir: PathBuf,
}

impl PayloadInfo {
    pub fn new(source: &PayloadMapping, config_dir_destination_path: &Path) -> PayloadInfo {
        PayloadInfo {
            code_revisions: source
                .code_mappings
                .iter()
                .filter_map(|code_mapping| match &code_mapping.source {
                    CodeSource::Remote { git_revision, .. } => {
                        Some((code_mapping.id.clone(), git_revision.clone()))
                    }
                    _ => None,
                })
                .collect::<HashMap<_, _>>(),
            config_dir: config_dir_destination_path.to_owned(),
        }
    }
}

pub fn build_payload_mapping(
    payload_mapping_config: &PayloadMappingConfig,
    config_dir_override_path: Option<&Path>,
    revisions: &HashMap<String, String>,
    config_base_dir: &Path,
) -> PayloadMapping {
    assert!(payload_mapping_config.config.entrypoint.is_relative());

    let config_dir_override_path = config_dir_override_path.map(|x| {
        x.is_relative()
            .then_some(config_base_dir.join(x))
            .unwrap_or(x.to_owned())
    });
    let config_dir_from_config = payload_mapping_config
        .config
        .dir
        .is_relative()
        .then_some(config_base_dir.join(payload_mapping_config.config.dir.clone()))
        .unwrap_or(payload_mapping_config.config.dir.clone());
    let config_dir_path = config_dir_override_path.unwrap_or(config_dir_from_config);

    let code_mappings: Vec<CodeMapping> = payload_mapping_config
        .code
        .iter()
        .map(|code_mapping_config| {
            assert!(code_mapping_config.target.is_relative());

            let source = if let Some(revision) = revisions.get(&code_mapping_config.id) {
                CodeSource::Remote {
                    url: code_mapping_config.remote.url.clone(),
                    git_revision: revision.to_owned(),
                }
            } else {
                CodeSource::Local {
                    path: code_mapping_config.local.path.clone(),
                    copy_excludes: code_mapping_config.local.excludes.clone(),
                }
            };

            CodeMapping {
                id: code_mapping_config.id.clone(),
                source,
                target: code_mapping_config.target.clone(),
            }
        })
        .collect();

    PayloadMapping {
        code_mappings,
        config_source: ConfigSource {
            entrypoint_path: payload_mapping_config.config.entrypoint.clone(),
            dir_path: config_dir_path,
        },
    }
}

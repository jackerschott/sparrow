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

impl CodeSource {
    pub fn git_revision(&self) -> Option<&String> {
        match self {
            CodeSource::Remote { git_revision, .. } => Some(git_revision),
            CodeSource::Local { .. } => None,
        }
    }
}

#[derive(Clone)]
pub struct CodeMapping {
    pub id: String,
    pub source: CodeSource,
    pub target_path: PathBuf,
}

#[derive(Clone)]
pub struct ConfigSource {
    pub entrypoint_path: PathBuf,
    pub dir_path: PathBuf,
}

#[derive(Clone)]
pub struct AuxiliaryMapping {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub copy_excludes: Vec<String>,
}

#[derive(Clone)]
pub struct PayloadMapping {
    pub code_mappings: Vec<CodeMapping>,
    pub config_source: ConfigSource,
    pub auxiliary_mappings: Vec<AuxiliaryMapping>,
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
    ignore_revisions: &Vec<String>,
    config_base_dir: &Path,
) -> PayloadMapping {
    assert!(payload_mapping_config.config.entrypoint.is_relative());

    ignore_revisions.iter().for_each(|ignore_id| {
        if !payload_mapping_config
            .code
            .iter()
            .any(|x| x.id == *ignore_id)
        {
            eprintln!(
                "cannot ignore revision of id `{}', not found in code mappings",
                ignore_id
            );
            std::process::exit(1);
        }

        if ignore_revisions
            .iter()
            .find(|&id| *id == *ignore_id)
            .is_some()
        {
            eprintln!("found duplicate id `{ignore_id}' in revision ignore request");
            std::process::exit(1);
        }
    });

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

            let source = if ignore_revisions
                .iter()
                .find(|id| *id == &code_mapping_config.id)
                .is_some()
            {
                CodeSource::Local {
                    path: code_mapping_config.local.path.clone(),
                    copy_excludes: code_mapping_config.local.excludes.clone().unwrap_or(vec![]),
                }
            } else {
                CodeSource::Remote {
                    url: code_mapping_config.remote.url.clone(),
                    git_revision: code_mapping_config.remote.revision.clone(),
                }
            };

            CodeMapping {
                id: code_mapping_config.id.clone(),
                source,
                target_path: code_mapping_config.target.clone(),
            }
        })
        .collect();

    let auxiliary_mappings = payload_mapping_config
        .auxiliary
        .clone()
        .unwrap_or(vec![])
        .iter()
        .map(|mapping_config| AuxiliaryMapping {
            source_path: mapping_config.path.clone(),
            target_path: mapping_config.target.clone(),
            copy_excludes: mapping_config.excludes.clone().unwrap_or(vec![]),
        })
        .collect();

    PayloadMapping {
        code_mappings,
        config_source: ConfigSource {
            entrypoint_path: payload_mapping_config.config.entrypoint.clone(),
            dir_path: config_dir_path,
        },
        auxiliary_mappings,
    }
}

use crate::cfg::PayloadMappingConfig;
use anyhow::{anyhow, Context, Result};
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
) -> Result<PayloadMapping> {
    assert!(payload_mapping_config.config.entrypoint.is_relative());

    for ignore_id in ignore_revisions.iter() {
        if !payload_mapping_config
            .code
            .keys()
            .any(|code_source_id| *code_source_id == *ignore_id)
        {
            return Err(anyhow!(
                "cannot ignore revision of id `{ignore_id}', not found in code mappings",
            ));
        }

        if ignore_revisions
            .iter()
            .filter(|&id| *id == *ignore_id)
            .count()
            > 1
        {
            return Err(anyhow!(
                "found duplicate id `{ignore_id}' in revision ignore request"
            ));
        }
    }

    if payload_mapping_config.config.dir.is_absolute() {
        return Err(anyhow!(
            "payload.config.dir is required to be relative, but got `{config_dir}` instead",
            config_dir = payload_mapping_config.config.dir
        ));
    }

    let config_dir_override_path = config_dir_override_path.map(|x| {
        x.is_relative()
            .then_some(config_base_dir.join(x))
            .unwrap_or(x.to_owned())
    });
    let config_dir_path = config_dir_override_path
        .unwrap_or(config_base_dir.join(payload_mapping_config.config.dir.clone()));

    let code_mappings: Vec<CodeMapping> = payload_mapping_config
        .code
        .iter()
        .map(|(code_source_id, code_mapping_config)| {
            assert!(code_mapping_config.target.is_relative());

            let source = if ignore_revisions
                .iter()
                .find(|id| **id == *code_source_id)
                .is_some()
            {
                // we always exclude the git directory, since this is never needed for runs
                let mut copy_excludes = vec![String::from("/.git/")];

                if !code_mapping_config.local.no_config_exclude {
                    copy_excludes.push(format!("/{}/", payload_mapping_config.config.dir));
                } else {
                    println!(
                        "warning: setting payload.code.{code_source_id}.local.no_config_exclude to true \
                        will be deprecated in future versions of sparrow, since it allows to copy the default \
                        config directory to the run directory; however the config might differ from the default \
                        directory, e.g. due to a config review, and thus the default config directory should never \
                        be used"
                    );
                }

                copy_excludes.extend(
                    read_excludes_from_gitignore()
                        .context("failed to add excludes from gitignore")?,
                );
                if let Some(exclude_additions) =
                    &code_mapping_config.local.gitignore_exclude_additions
                {
                    copy_excludes.extend(exclude_additions.clone());
                }
                if let Some(exclude_subtractions) =
                    &code_mapping_config.local.gitignore_exclude_subtractions
                {
                    copy_excludes.retain(|pattern| !exclude_subtractions.contains(pattern));
                }

                CodeSource::Local {
                    path: code_mapping_config.local.path.clone(),
                    copy_excludes,
                }
            } else {
                CodeSource::Remote {
                    url: code_mapping_config.remote.url.clone(),
                    git_revision: code_mapping_config.remote.revision.clone(),
                }
            };

            Ok(CodeMapping {
                id: code_source_id.clone(),
                source,
                target_path: code_mapping_config.target.clone(),
            })
        })
        .collect::<Result<_>>()?;

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

    Ok(PayloadMapping {
        code_mappings,
        config_source: ConfigSource {
            entrypoint_path: payload_mapping_config.config.entrypoint.clone(),
            dir_path: config_dir_path,
        },
        auxiliary_mappings,
    })
}

fn read_excludes_from_gitignore() -> Result<Vec<String>> {
    Ok(std::fs::read_to_string(".gitignore")
        .context("failed to open `.gitignore', are you in the project root?")?
        .lines()
        .filter(|line| !line.starts_with("#") && !line.is_empty())
        .map(String::from)
        .collect())
}

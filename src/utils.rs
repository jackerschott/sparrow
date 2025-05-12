use anyhow::{bail, Context, Result};
use camino::Utf8Path as Path;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};
use walkdir::DirEntry;

pub trait AsUtf8Path {
    fn as_utf8(&self) -> &Path;
}

impl AsUtf8Path for std::path::Path {
    fn as_utf8(&self) -> &Path {
        Path::from_path(self).expect("directory entry path is not a valid utf8 string")
    }
}

pub trait Utf8Path {
    fn utf8_path(&self) -> &Path;
}

impl Utf8Path for TempDir {
    fn utf8_path(&self) -> &Path {
        Path::from_path(self.path()).expect("directory entry path is not a valid utf8 string")
    }
}

impl Utf8Path for NamedTempFile {
    fn utf8_path(&self) -> &Path {
        Path::from_path(self.path()).expect("directory entry path is not a valid utf8 string")
    }
}

impl Utf8Path for DirEntry {
    fn utf8_path(&self) -> &Path {
        Path::from_path(self.path()).expect("directory entry path is not a valid utf8 string")
    }
}

pub trait Utf8Str {
    fn utf8_str(&self) -> &str;
}

impl Utf8Str for std::ffi::OsString {
    fn utf8_str(&self) -> &str {
        self.to_str().expect("expected os string to be valid utf8")
    }
}

impl Utf8Str for std::ffi::OsStr {
    fn utf8_str(&self) -> &str {
        self.to_str().expect("expected os string to be valid utf8")
    }
}

pub fn select_interactively<'d, D: std::fmt::Display>(
    options: &'d Vec<D>,
    prompt: &str,
) -> Result<&'d D> {
    let mut fzf_command = std::process::Command::new("fzf");

    println!("options: {:?}", options.iter().map(|option| format!("{}", option)).collect::<Vec<_>>());

    fzf_command
        .arg("--prompt")
        .arg(prompt)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());

    let mut child = fzf_command
        .spawn()
        .context(format!("failed to spawn interactive selection command `{fzf_command:?}`"))?;

    let options_input = options
        .iter()
        .map(|option| option.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    child
        .stdin
        .as_mut()
        .expect("expected stdin of fzf to be piped before")
        .write_all(options_input.as_bytes())
        .context(format!("failed to write to stdin of interactive selection `{fzf_command:?}`"))?;

    let output = child
        .wait_with_output()
        .context(format!("failed to wait for output of interactive selection `{fzf_command:?}`"))?;
    if !output.status.success() {
        bail!("interactive selection failed to exit successfully, most likely because nothing was selected");
    }

    let output = String::from_utf8(output.stdout).context(format!(
        "found non-valid utf8 in output of `{fzf_command:?}` "
    ))?;
    let output = output.trim();

    return Ok(
        options
            .iter()
            .find(|x| x.to_string() == output)
            .expect("expected rofi output to be one of the options"),
    );
}

pub fn tmux_wrap(cmd: &str, session_name: &str) -> String {
    let cmd = escape_single_quotes(cmd);
    return format!("exec tmux new-session -s {session_name} '{cmd}; bash'");
}

pub fn escape_single_quotes(cmd: &str) -> String {
    return cmd.replace("'", "'\"'\"'");
}

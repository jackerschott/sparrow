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

pub fn select_interactively<D: std::fmt::Display>(options: &Vec<D>) -> &D {
    let mut child = std::process::Command::new("rofi")
        .arg("-dmenu")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("expected rofi to start successfully");

    let options_input = options
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    child
        .stdin
        .as_mut()
        .expect("expected stdin of rofi to be piped")
        .write_all(options_input.as_bytes())
        .expect("expected write to stdin of rofi to work");

    let output = child
        .wait_with_output()
        .expect("expected waiting for rofi output to work");
    if !output.status.success() {
        std::process::exit(1);
    }

    let output = String::from_utf8(output.stdout).expect("expected rofi output to be valid utf8");
    let output = output.trim();

    return options
        .iter()
        .find(|x| x.to_string() == output)
        .expect("expected rofi output to be one of the options");
}

pub fn tmux_wrap(cmd: &str, session_name: &str) -> String {
    let cmd = escape_single_quotes(cmd);
    return format!("exec tmux new-session -s {session_name} '{cmd}; bash'");
}

pub fn escape_single_quotes(cmd: &str) -> String {
    return cmd.replace("'", "'\"'\"'");
}

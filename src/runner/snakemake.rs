use super::super::host::Host;
use super::Runner;
use camino::Utf8Path as Path;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_run_script(
    experiment_name: &str,
    experiment_group: &str,
    experiment_base_dir_path: &Path,
    code_revision: Option<&str>,
    host_id: &str,
    host_is_local: bool,
    host_remote_run_is_test: bool,
    runner_cmdline: Vec<String>,
) -> NamedTempFile {
    // load file as string
    let run_template_content =
        std::fs::read_to_string("run.sh.j2").expect("couldn't find run.sh.j2 in current directory");

    let mut env = minijinja::Environment::new();
    env.add_template("run", run_template_content.as_str())
        .unwrap();
    let run_template = env.get_template("run").unwrap();
    let run_script = run_template
        .render(minijinja::context! {
            experiment_name => experiment_name,
            experiment_group => experiment_group,
            experiment_base_dir => experiment_base_dir_path,
            code_revision => code_revision,
            host => host_id,
            is_local => host_remote_run_is_test,
            is_test => host_remote_run_is_test || host_is_local,
            extra_flags => runner_cmdline.join(" "),
        })
        .expect("expted run script template rendering to work");

    let mut run_script_file =
        NamedTempFile::new().expect("could not create temporary run script file");
    run_script_file
        .write(run_script.as_bytes())
        .expect("could not write to temporary run script file");
    return run_script_file;
}

pub fn run<H: Host>(host: H, experiment_name: &str, experiment_group: &str) {}

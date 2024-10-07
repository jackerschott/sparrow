use super::{ExperimentInfo, Runner};
use crate::host::{ExperimentID, Host, RunDirectory};
use crate::utils::{escape_single_quotes, tmux_wrap};
use std::io::Write;
use std::os::unix::process::CommandExt;
use tempfile::NamedTempFile;

pub struct Snakemake {
    cmdline: Vec<String>,
}

impl Snakemake {
    pub fn new(cmdline: &Vec<String>) -> Self {
        return Self {
            cmdline: cmdline.clone(),
        };
    }
}

impl Runner for Snakemake {
    fn create_run_script(&self, experiment_info: &ExperimentInfo) -> NamedTempFile {
        let context = build_template_context(experiment_info);

        // load file as string
        let run_template_content = std::fs::read_to_string("run.sh.j2")
            .expect("couldn't find run.sh.j2 in current directory");

        let mut env = minijinja::Environment::new();
        env.add_template("run", run_template_content.as_str())
            .unwrap();
        let run_template = env.get_template("run").unwrap();
        let run_script_content = run_template
            .render(context)
            .expect("expected run script template rendering to work");

        let mut run_script =
            NamedTempFile::new().expect("could not create temporary run script file");
        run_script
            .write(run_script_content.as_bytes())
            .expect("could not write to temporary run script file");
        return run_script;
    }

    fn run(&self, host: &dyn Host, run_dir: &RunDirectory, experiment_id: &ExperimentID) {
        let run_cmd = &format!("cd {} && bash ./run.sh", run_dir.path());

        let shell = std::env::var("SHELL").unwrap();
        let mut cmd = std::process::Command::new(shell);
        cmd.arg("-c");

        if host.is_local() {
            cmd.arg(run_cmd).exec();
            return;
        }

        let hostname = host.hostname();
        let tmux_session_name = &format!("{experiment_id}");
        let run_cmd_wrapped = tmux_wrap(run_cmd, tmux_session_name);
        let run_cmd_wrapped = escape_single_quotes(&run_cmd_wrapped);
        cmd.arg(&format!(
            "ssh -tt {hostname} 'cd {} && {run_cmd_wrapped}'",
            run_dir.path()
        ))
        .exec();
    }

    fn cmdline(&self) -> &Vec<String> {
        return &self.cmdline;
    }
}

fn build_template_context(experiment_info: &ExperimentInfo) -> minijinja::Value {
    minijinja::context! {
        experiment_id => experiment_info.id,
        host => experiment_info.host,
        runner => experiment_info.runner,
        payload_source => experiment_info.payload_source,
    }
}

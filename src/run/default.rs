use super::{RunInfo, Runner};
use crate::host::{Host, RunDirectory, RunID};
use crate::utils::{escape_single_quotes, tmux_wrap};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::process::CommandExt;
use tempfile::NamedTempFile;

pub struct DefaultRunner {
    cmdline: Vec<String>,
    environment_variable_transfer_requests: Vec<String>,
    config: HashMap<String, String>,
}

impl DefaultRunner {
    pub fn new(
        cmdline: &Vec<String>,
        environment_variable_transfer_requests: &Vec<String>,
        config: &HashMap<String, String>,
    ) -> Self {
        return Self {
            cmdline: cmdline.clone(),
            environment_variable_transfer_requests: environment_variable_transfer_requests.clone(),
            config: config.clone(),
        };
    }
}

impl Runner for DefaultRunner {
    fn create_run_script(&self, run_info: &RunInfo) -> NamedTempFile {
        let context = build_template_context(run_info);

        // load file as string
        let run_template_content = std::fs::read_to_string(".sparrow/run.sh.j2")
            .expect("couldn't find .sparrow/run.sh.j2 in current directory");

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

    fn run(&self, host: &dyn Host, run_dir: &RunDirectory, run_id: &RunID) {
        let run_cmd = &format!(
            "cd {run_dir_path} && {script_run_command}",
            run_dir_path = run_dir.path(),
            script_run_command = host.script_run_command("./run.sh")
        );

        let shell = std::env::var("SHELL").unwrap();
        let mut cmd = std::process::Command::new(shell);
        cmd.arg("-c");

        let environment_variables_to_transfer = self
            .environment_variable_transfer_requests
            .iter()
            .map(|variable_name| {
                let variable_value = std::env::var(variable_name).expect(
                    "expected variable to be retreivable from the environment \
                        due to a previous check when building the runner",
                );
                (variable_name, variable_value)
            })
            .collect::<Vec<_>>();

        if host.is_local() {
            cmd.arg(run_cmd).exec();
            return;
        }

        let hostname = host.hostname();
        let tmux_session_name = &format!("{run_id}");
        let run_cmd_wrapped = tmux_wrap(run_cmd, tmux_session_name);
        let run_cmd_wrapped = escape_single_quotes(&run_cmd_wrapped);

        let run_cmd_wrapped_with_variables = format!(
            "{} {run_cmd_wrapped}",
            environment_variables_to_transfer
                .iter()
                .map(|(name, value)| { escape_single_quotes(&format!("{name}='{value}'")) })
                .collect::<Vec<_>>()
                .join(" ")
        );
        cmd.arg(&format!(
            "ssh -qtt {hostname} 'cd {} && {run_cmd_wrapped_with_variables}'",
            run_dir.path()
        ))
        .exec();
    }

    fn cmdline(&self) -> &Vec<String> {
        return &self.cmdline;
    }

    fn config(&self) -> &HashMap<String, String> {
        return &self.config;
    }
}

fn build_template_context(run_info: &RunInfo) -> minijinja::Value {
    minijinja::context! {
        run_id => run_info.id,
        host => run_info.host,
        runner => run_info.runner,
        payload => run_info.payload,
        output_path => run_info.output_path,
    }
}

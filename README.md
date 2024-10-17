# Runner

[![rust](https://img.shields.io/badge/rust-1.82.0--nightly-blue?logo=rust)](https://www.rust-lang.org/)

## Introduction

This repository contains a simple runner program for running arbitrary payloads
in an HPC cluster environment.
It is designed to completely abstract the running environment, such that local
testing, testing on a cluster and doing a full cluster run requires nothing more
than changing a simple cli flag.
It also handles node allocation, for performing quick (test) runs on a cluster
and various commands that allow for easy access to log files and the outputs of
a run.

It works by providing all information about your project in a `run.yaml` file in
your project root directory, which for example looks like this
```yaml
experiment_group: test
runner: snakemake

code_source:
  local:
    path: /home/jona/studies/phd/runner_rust/repo
    excludes:
      - .git/
      - __pycache__/
      - experiments/
      - results/
      - "*.egg-info/"
      - run.yaml
      - .snakemake/
      - .ruff_cache/
      - .config/
      - .cache/
  remote:
    url: "ssh://git@gitlab.cern.ch:7999/jackersc/runner-test.git"

  config:
    dir: config
    entrypoint: config.yaml

remote_host:
  host_type: slurm-cluster
  id: baobab
  hostname: baobab
  experiment_base_dir: /srv/beegfs/scratch/users/a/ackersch/projects/bla/experiments
  temporary_dir: /srv/beegfs/scratch/users/a/ackersch/tmp
  quick_run:
    time: 5:00:00
    cpu_count: 4
    gpu_count: 0
    fast_access_container_requests: []

local_host:
  experiment_base_dir: /home/jona/studies/phd/runner_rust/repo/experiments

experiment_sync_options:
  result_excludes: []
  model_excludes: []

results:
  - results/bla.pdf
```
This file contains information about where to find the code and the
configuration, where and how to run the code and how the outputs are stored.
The runner then uploads the repository to the remote host, allows the user to
review and potentially change the config directory before the run and executes
a the code.
The code execution works by rendering a template `run.sh.j2` file (which
should be present in the project root) to a `run.sh` file which gets executed
by bash, potentially in a tmux session. A typical `run.sh.j2` file, using
snakemake, would look like this:
```jinja2
{%- if not host.is_local -%}
module load GCCcore/13.2.0 Python/3.11.5

{% endif -%}
snakemake \
{%- if "--snakefile=" not in runner.cmdline %}
    --snakefile=workflow/Snakefile \
{%- endif %}
    --workflow-profile=workflow/profiles/{{ host.id + "-test" if host.is_configured_for_quick_run and not host.is_local else host.id }} \
    --config \
        experiment_name={{ experiment_id.name }} \
        experiment_group={{ experiment_id.group }} \
        experiment_base_dir={{ host.experiment_base_dir_path }} \
        code_revision={{ payload.code_revision }} \
        host={{ host.id }} \
        devstage={{ 'test' if host.is_local or host.is_configured_for_quick_run else 'experiment' }} \
        config_dir={{ payload.config_dir }}
    {{ runner.cmdline }}
```
The template rendering takes care to propagate all variables from the runner
to the run script which are available in jinja (the template rendering engine).
Executing the run script within a tmux session allows the user to easily detach
from the run and reattach to it later.

## Getting Started
To get started, simply build and install the project with
```bash
cargo install --path .
```
Take care that the cargo installation directory is in your path.
Afterwards, simply run
```bash
runner --help
```
to see the available commands and options.

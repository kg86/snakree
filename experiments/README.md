# Experiments

This directory holds the experiments run on this repository, such as benchmark runs for the proposals in `IDEAS.md`.
Follow these rules every time an experiment is run.

## Directory per experiment

Create one directory per experiment and put every generated file in it.

```
experiments/yyyy-mm-dd-xx/
```

- `yyyy-mm-dd` is the date on which the experiment was run.
- `xx` is a two-digit number, starting at `01` and increasing for each experiment run on the same date.
  Use the smallest number that is not yet taken.
- Result files, charts, logs and scripts written for the experiment all go in this directory, not in the repository root or in `/tmp`.
- Do not reuse or overwrite the directory of an earlier experiment.
  Run a new experiment in a new directory.

## README.md in each directory

Every experiment directory contains a `README.md` that records:

- the purpose, that is, the question the experiment answers;
- the method, that is, the exact commands, options, inputs, revisions and environment used, so that the experiment can be repeated (see below);
- the results, that is, what was observed, with links to the result files and charts in the directory.

## Scenarios

A scenario is a named, reusable set of benchmark options.
It is defined in `benches/scenarios/NAME.sh` and run with `benches/scripts/run-scenario.sh NAME experiments/yyyy-mm-dd-xx`.
The runner writes `scenario.txt` (the command), `env.txt`, `corpora-check.txt`, `result.tsv` and `charts/` into the experiment directory.
The experiment README names the scenario and still lists its parameters, because the scenario file can change later; the commit hash records the version that was run.
Scenario documentation can include a curated, self-contained example under `benches/examples/`, with its report, figures, numerical summaries, and source commit identifiers committed together on `main`.
Use relative links within the example, and retain full measurement archives on `experiments`.
The example illustrates the output and interpretation rather than defining expected timings on another machine.

Scenarios:

- `silesia-quick`: every benchmark scenario on the 12 Silesia files cut to 4 MiB, windows of 64 KiB and 1 MiB, pattern lengths 1 to 64, and `--reps 1`.
  It is for trying out the experiment process quickly, not for accurate numbers.

## Figures in the experiment README

Show the figures that were generated in the experiment directory in the README, as Markdown images with relative links, for example `![Sliding throughput](charts/scaling.svg)`.
The README can then be read with its figures in a Markdown viewer, and each figure is followed by a short explanation of what it shows.

## Reproducibility record

The method section of the README records:

- the branch name and the hash of the commit that has the code that was run;
- the versions of `rustc` and the operating system, and the CPU and memory of the machine;
- the commands that were run, copied as they were typed;
- the source of every corpus and its checksum, for example by naming `benches/corpora.sha256` and the download script used.

## Measurement

- Use `--reps 3` unless there is a reason to use another value.
  A pilot run may use `--reps 1`.
- Do not run other heavy processes at the same time.
- Describe results from few repetitions, and in particular from a pilot run, as trends, and do not discuss small differences.

Write the purpose and the method before running the experiment, and add the results after it finishes.

## Version control

The rules in this section apply only to experiments.
Changes that are not part of an experiment, such as code, tooling and documentation, follow `AGENTS.md`.

All experiments are kept on one long-lived branch, `experiments`.

- Run every experiment on the `experiments` branch, on committed code.
- Merge committed code from `main` into `experiments` (`git merge main`) before measuring it.
  If proposed code needs measurements before adoption on `main`, commit it on a topic branch and merge that branch into `experiments` before the run.
  A topic branch for measurement does not require a pull request unless the user requests one.
  Do not rebase `experiments`, so that the commit hashes recorded in the experiment READMEs stay valid.
- If an experiment needs code that is not meant for `main`, such as temporary instrumentation, commit it on `experiments` in its own commit before the run.
- After the run, commit the experiment directory in its own commit, for example `Add experiment yyyy-mm-dd-xx`.
  The results may be committed there, but do not commit large files such as corpora or long logs.
- Never merge `experiments` into `main`, and never commit `experiments/yyyy-mm-dd-xx/` on `main`.
  Only this `README.md` is tracked on `main`.
  To bring code that was written on `experiments` to `main`, cherry-pick its commit.
- The README of the experiment records the branch name `experiments` and the hash of the commit that has the code that was run, that is, the head of `experiments` before the commit of the experiment directory.

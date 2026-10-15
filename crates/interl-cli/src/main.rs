use clap::{Parser, Subcommand};
use interl_engine::{CancellationToken, PreparedRun, RunOptions};
use std::{future::Future, path::PathBuf, process::ExitCode, task::Poll};

#[derive(Parser)]
#[command(
    name = "interl",
    version,
    about = "Visual API concurrency and correctness testing"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Validate a scenario file.
    Validate { scenario: PathBuf },
    /// Execute one user's sequential HTTP and authentication workflow.
    Run {
        scenario: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}
#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Commands::Validate { scenario } => match interl_core::open_document(&scenario) {
            Ok(result) if result.validation.valid => {
                println!("Valid scenario: {}", scenario.display());
                ExitCode::SUCCESS
            }
            Ok(result) => {
                for diagnostic in result.validation.diagnostics {
                    eprintln!("{diagnostic}");
                }
                ExitCode::from(2)
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        },
        Commands::Run { scenario, output } => run(scenario, output).await,
    }
}
async fn run(scenario: PathBuf, output: Option<PathBuf>) -> ExitCode {
    if let Some(path) = &output {
        // Never overwrite the input, including an existing symlink to the input.
        if std::fs::canonicalize(&scenario)
            .ok()
            .zip(std::fs::canonicalize(path).ok())
            .is_some_and(|(a, b)| a == b)
        {
            eprintln!("Report output must differ from the scenario input");
            return ExitCode::from(2);
        }
    }
    let document = match interl_core::open_document(&scenario) {
        Ok(result) if result.validation.valid => {
            result.validation.document.expect("valid document")
        }
        Ok(result) => {
            for diagnostic in result.validation.diagnostics {
                eprintln!("{diagnostic}");
            }
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let prepared = match PreparedRun::new(&document, RunOptions::default()) {
        Ok(run) => run,
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            return ExitCode::from(2);
        }
    };
    let cancel = CancellationToken::new();
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    // Registration happens on first poll, not future construction. Arm the OS
    // handler before starting HTTP so an immediate SIGINT cannot terminate the
    // process before it can write a safe partial report.
    let initial = std::future::poll_fn(|cx| Poll::Ready(interrupt.as_mut().poll(cx))).await;
    let mut interrupted = match initial {
        Poll::Pending => false,
        Poll::Ready(result) => {
            if result.is_err() {
                eprintln!("Interrupt handler unavailable; cancelling run");
            }
            cancel.cancel();
            true
        }
    };
    let mut handle = prepared.start(cancel.clone());
    loop {
        tokio::select! {
            result = &mut interrupt, if !interrupted => {
                interrupted = true;
                if result.is_err() { eprintln!("Interrupt handler unavailable; cancelling run"); }
                cancel.cancel();
            },
            event = handle.events.recv() => match event {
                Some(event) => {
                    let wire = serde_json::to_value(&event).expect("event is serializable");
                    // Human output intentionally excludes arbitrary node names, URLs and bodies.
                    println!("{} {}", event.sequence, wire["type"].as_str().unwrap_or("event"));
                },
                None => break,
            }
        }
    }
    let report = match handle.task.await {
        Ok(report) => report,
        Err(_) => {
            eprintln!("Execution task failed");
            return ExitCode::from(3);
        }
    };
    if let Some(output) = output {
        let json = serde_json::to_vec(&report).expect("report is serializable");
        if std::fs::write(output, json).is_err() {
            eprintln!("Report could not be written; check the output path and permissions");
            return ExitCode::from(3);
        }
    }
    if let Some(error) = &report.error {
        eprintln!("{}: {}", error.kind, error.message);
    }
    println!(
        "Run {:?}: {} requests, {} responses",
        report.status, report.summary.requests, report.summary.responses
    );
    ExitCode::from(interl_engine::exit_code(report.status))
}

use std::path::PathBuf;

use anyhow::Result;
use caboodle::{
    engine,
    model::{Plan, Profile},
};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Write a reviewable install plan without changing the machine
    Plan {
        #[arg(long, value_enum, default_value_t = ProfileArg::Retrieval)]
        profile: ProfileArg,
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        output: PathBuf,
    },
    /// Converge installed tools and prove each binary by version read-back
    Apply {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        #[arg(long, default_value = ".caboodle/state.json")]
        state: PathBuf,
        /// Refuse to install missing tools; useful for managed or offline machines
        #[arg(long)]
        skip_install: bool,
    },
    /// Run isolated functional round trips for every selected tool
    Verify {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        #[arg(long, default_value = ".caboodle/state.json")]
        state: PathBuf,
    },
    /// Apply and verify a previously reviewed plan
    Install {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        #[arg(long, default_value = ".caboodle/state.json")]
        state: PathBuf,
        #[arg(long)]
        skip_install: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileArg {
    Kg,
    Retrieval,
}

impl From<ProfileArg> for Profile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Kg => Self::Kg,
            ProfileArg::Retrieval => Self::Retrieval,
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Plan { profile, output } => {
            Plan::for_profile(profile.into()).write(&output)?;
            println!("plan: {}", output.display());
        }
        Commands::Apply {
            plan,
            state,
            skip_install,
        } => {
            engine::apply(&Plan::read(&plan)?, &state, skip_install)?;
        }
        Commands::Verify { plan, state } => {
            engine::verify(&Plan::read(&plan)?, &state)?;
        }
        Commands::Install {
            plan,
            state,
            skip_install,
        } => {
            let plan = Plan::read(&plan)?;
            engine::apply(&plan, &state, skip_install)?;
            engine::verify(&plan, &state)?;
        }
    }
    Ok(())
}

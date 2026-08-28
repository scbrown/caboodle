use std::path::PathBuf;

use anyhow::Result;
use caboodle::{
    crew::CrewEvidence,
    emission, engine, interview,
    model::{CrewMode, InstallIntent, Plan, Profile},
    observability, projection,
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
    /// Run the resumable guided interview and write a reviewable plan
    Init {
        /// Use the guided question flow
        #[arg(long)]
        guided: bool,
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        output: PathBuf,
        #[arg(long, default_value = ".caboodle/interview.toml")]
        session: PathBuf,
    },
    /// Write a reviewable install plan without changing the machine
    Plan {
        #[arg(long, value_enum, default_value_t = ProfileArg::Retrieval)]
        profile: ProfileArg,
        /// Crew runtime when --profile crew is selected
        #[arg(long, value_enum, default_value_t = CrewModeArg::Standalone)]
        crew: CrewModeArg,
        /// Reviewable intended-use, crew-theme, and ontology-question contract
        #[arg(long, default_value = "caboodle-intent.toml")]
        intent: PathBuf,
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
        /// Creel browser doctor JSON produced by its capability preflight
        #[arg(long)]
        creel_doctor: Option<PathBuf>,
        /// Creel admission JSON produced by its provider-window governor
        #[arg(long)]
        creel_admission: Option<PathBuf>,
    },
    /// Apply and verify a previously reviewed plan
    Install {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        #[arg(long, default_value = ".caboodle/state.json")]
        state: PathBuf,
        #[arg(long)]
        skip_install: bool,
        #[arg(long)]
        creel_doctor: Option<PathBuf>,
        #[arg(long)]
        creel_admission: Option<PathBuf>,
    },
    /// Project one reviewed crew policy through harness-owned settings adapters
    ProjectSettings {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        #[arg(short, long, default_value = "caboodle-settings")]
        output: PathBuf,
    },
    /// Execute every anticipated ontology question against the installed Quipu
    VerifyQuestions {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        /// Optional isolated Quipu database used by the question probes
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Render reviewable Prometheus, alert, dashboard, and contract artifacts
    RenderObservability {
        #[arg(short, long, default_value = "caboodle-plan.toml")]
        plan: PathBuf,
        #[arg(long, default_value = "caboodle-observability.toml")]
        targets: PathBuf,
        #[arg(short, long, default_value = "caboodle-observability")]
        output: PathBuf,
    },
    /// Validate generated observability artifacts against their contracts
    ValidateObservability {
        #[arg(short, long, default_value = "caboodle-observability")]
        output: PathBuf,
    },
    /// Queue created/commented/closed lifecycle episodes from a br JSONL snapshot
    QueueBr {
        input: PathBuf,
        #[arg(short, long, default_value = ".caboodle/episodes")]
        queue: PathBuf,
    },
    /// Deliver identical queued episodes after a Quipu readiness control passes
    FlushEpisodes {
        #[arg(short, long, default_value = ".caboodle/episodes")]
        queue: PathBuf,
        #[arg(long)]
        endpoint: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileArg {
    Kg,
    Retrieval,
    CodeIntel,
    Crew,
    Everything,
}

#[derive(Clone, Copy, ValueEnum)]
enum CrewModeArg {
    Shantytown,
    Creel,
    Both,
    Standalone,
}

impl From<CrewModeArg> for CrewMode {
    fn from(value: CrewModeArg) -> Self {
        match value {
            CrewModeArg::Shantytown => Self::Shantytown,
            CrewModeArg::Creel => Self::Creel,
            CrewModeArg::Both => Self::Both,
            CrewModeArg::Standalone => Self::Standalone,
        }
    }
}

impl From<ProfileArg> for Profile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Kg => Self::Kg,
            ProfileArg::Retrieval => Self::Retrieval,
            ProfileArg::CodeIntel => Self::CodeIntel,
            ProfileArg::Crew => Self::Crew,
            ProfileArg::Everything => Self::Everything,
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Init {
            guided,
            output,
            session,
        } => {
            if !guided {
                anyhow::bail!("caboodle init currently requires --guided");
            }
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            let stdout = std::io::stdout();
            let mut output_stream = stdout.lock();
            interview::guided(&mut input, &mut output_stream, &session, &output)?;
        }
        Commands::Plan {
            profile,
            crew,
            intent,
            output,
        } => {
            let profile = Profile::from(profile);
            let mut plan = if profile == Profile::Crew {
                Plan::for_crew(crew.into())
            } else {
                Plan::for_profile(profile)
            };
            plan.intent = Some(InstallIntent::read(&intent)?);
            plan.write(&output)?;
            println!("plan: {}", output.display());
        }
        Commands::Apply {
            plan,
            state,
            skip_install,
        } => {
            engine::apply(&Plan::read(&plan)?, &state, skip_install)?;
        }
        Commands::Verify {
            plan,
            state,
            creel_doctor,
            creel_admission,
        } => {
            engine::verify(
                &Plan::read(&plan)?,
                &state,
                &CrewEvidence {
                    creel_doctor,
                    creel_admission,
                },
            )?;
        }
        Commands::Install {
            plan,
            state,
            skip_install,
            creel_doctor,
            creel_admission,
        } => {
            let plan = Plan::read(&plan)?;
            engine::apply(&plan, &state, skip_install)?;
            engine::verify(
                &plan,
                &state,
                &CrewEvidence {
                    creel_doctor,
                    creel_admission,
                },
            )?;
        }
        Commands::ProjectSettings { plan, output } => {
            for name in projection::write(&Plan::read(&plan)?, &output)? {
                println!("settings: {}", output.join(name).display());
            }
        }
        Commands::VerifyQuestions { plan, db } => {
            engine::verify_questions(&Plan::read(&plan)?, db.as_deref())?;
        }
        Commands::RenderObservability {
            plan,
            targets,
            output,
        } => {
            observability::render(&Plan::read(&plan)?, &targets, &output)?;
            println!("observability: {}", output.display());
        }
        Commands::ValidateObservability { output } => {
            observability::validate(&output)?;
            println!("observability contracts: verified");
        }
        Commands::QueueBr { input, queue } => {
            let count = emission::queue_br_jsonl(&input, &queue)?;
            println!("queued lifecycle projections: {count}");
        }
        Commands::FlushEpisodes { queue, endpoint } => {
            let count = emission::flush(&queue, &endpoint)?;
            println!("delivered queued episodes: {count}");
        }
    }
    Ok(())
}

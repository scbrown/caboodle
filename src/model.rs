use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Kg,
    Retrieval,
    CodeIntel,
    Crew,
    Everything,
}

impl Profile {
    pub fn tools(self) -> Vec<ToolName> {
        match self {
            Self::Kg => vec![ToolName::Quipu, ToolName::Camayoc],
            Self::Retrieval => vec![ToolName::Quipu, ToolName::Camayoc, ToolName::Bobbin],
            Self::CodeIntel => vec![
                ToolName::Quipu,
                ToolName::Camayoc,
                ToolName::Bobbin,
                ToolName::Yupana,
            ],
            Self::Crew => vec![ToolName::Quipu, ToolName::Camayoc, ToolName::Bobbin],
            Self::Everything => vec![
                ToolName::Quipu,
                ToolName::Camayoc,
                ToolName::Bobbin,
                ToolName::Yupana,
                ToolName::DesirePath,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrewMode {
    Shantytown,
    Creel,
    Both,
    Standalone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrewOwner {
    Shantytown,
    Creel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrewRouting {
    SingleOwner,
    ExplicitHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewSelection {
    pub mode: CrewMode,
    pub durable_owner: Option<CrewOwner>,
    pub burst_owner: Option<CrewOwner>,
    pub routing: Option<CrewRouting>,
    pub policy: CrewPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewPolicy {
    pub identity_source: IdentitySource,
    pub model: Option<String>,
    pub tools: Vec<ToolName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentitySource {
    Quipu,
}

impl CrewPolicy {
    fn standard() -> Self {
        Self {
            identity_source: IdentitySource::Quipu,
            model: None,
            tools: Profile::Crew.tools(),
        }
    }

    fn validate(&self) -> Result<()> {
        if self.tools != Profile::Crew.tools() {
            bail!("crew policy tools must match the crew profile conventions");
        }
        if self
            .model
            .as_ref()
            .is_some_and(|model| model.trim().is_empty())
        {
            bail!("crew policy model must be omitted or non-empty");
        }
        Ok(())
    }
}

impl CrewSelection {
    pub fn for_mode(mode: CrewMode) -> Self {
        match mode {
            CrewMode::Shantytown => Self {
                mode,
                durable_owner: Some(CrewOwner::Shantytown),
                burst_owner: None,
                routing: Some(CrewRouting::SingleOwner),
                policy: CrewPolicy::standard(),
            },
            CrewMode::Creel => Self {
                mode,
                durable_owner: None,
                burst_owner: Some(CrewOwner::Creel),
                routing: Some(CrewRouting::SingleOwner),
                policy: CrewPolicy::standard(),
            },
            CrewMode::Both => Self {
                mode,
                durable_owner: Some(CrewOwner::Shantytown),
                burst_owner: Some(CrewOwner::Creel),
                routing: Some(CrewRouting::ExplicitHandoff),
                policy: CrewPolicy::standard(),
            },
            CrewMode::Standalone => Self {
                mode,
                durable_owner: None,
                burst_owner: None,
                routing: None,
                policy: CrewPolicy::standard(),
            },
        }
    }

    fn validate(&self) -> Result<()> {
        let mut expected = Self::for_mode(self.mode);
        expected.policy = self.policy.clone();
        if self != &expected {
            bail!(
                "crew {:?} must use the declared ownership/routing contract; expected {:?}",
                self.mode,
                expected
            );
        }
        self.policy.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolName {
    Quipu,
    Camayoc,
    Bobbin,
    Yupana,
    #[serde(rename = "desire-path")]
    DesirePath,
}

impl ToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quipu => "quipu",
            Self::Camayoc => "camayoc",
            Self::Bobbin => "bobbin",
            Self::Yupana => "yupana",
            Self::DesirePath => "desire-path",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuipuFlavor {
    /// The reviewed release feature set, which deliberately excludes the
    /// `lancedb` cargo feature — `vector.backend = "lancedb"` refuses at
    /// startup on a box installed this way.
    #[default]
    Release,
    /// The same reviewed revision built with the `lancedb` feature added.
    /// Only the feature list changes, so this flavor cannot drift to an
    /// unreviewed Quipu.
    Lancedb,
}

impl QuipuFlavor {
    /// The default flavor is omitted from serialized plans so that plans
    /// written before this field existed and plans written today stay
    /// byte-identical.
    fn is_release(&self) -> bool {
        *self == Self::Release
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub profile: Profile,
    pub tools: Vec<ToolName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quipu_db: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "QuipuFlavor::is_release")]
    pub quipu_flavor: QuipuFlavor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crew: Option<CrewSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<InstallIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallIntent {
    pub intended_use: String,
    #[serde(default)]
    pub crew_members: Vec<CrewMemberTheme>,
    pub anticipated_questions: Vec<QuestionContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewMemberTheme {
    pub name: String,
    pub theme: String,
    pub domain: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionContract {
    pub question: String,
    pub answer_shape: String,
    pub seed_intent: String,
    pub sparql: String,
    pub expected: String,
}

impl InstallIntent {
    pub fn read(path: &Path) -> Result<Self> {
        let body =
            fs::read_to_string(path).with_context(|| format!("read intent {}", path.display()))?;
        let intent: Self =
            toml::from_str(&body).with_context(|| format!("parse intent {}", path.display()))?;
        intent.validate()?;
        Ok(intent)
    }

    pub fn validate(&self) -> Result<()> {
        require_safe_text("intended use", &self.intended_use)?;
        if self.anticipated_questions.is_empty() {
            bail!("intent requires at least one anticipated ontology question");
        }
        let mut names = std::collections::BTreeSet::new();
        for member in &self.crew_members {
            for (field, value) in [
                ("crew member name", &member.name),
                ("crew member theme", &member.theme),
                ("crew member domain", &member.domain),
                ("crew member role", &member.role),
            ] {
                require_safe_text(field, value)?;
            }
            if !names.insert(member.name.to_ascii_lowercase()) {
                bail!("duplicate crew member name {:?}", member.name);
            }
        }
        let mut questions = std::collections::BTreeSet::new();
        for contract in &self.anticipated_questions {
            for (field, value) in [
                ("anticipated question", &contract.question),
                ("answer shape", &contract.answer_shape),
                ("seed intent", &contract.seed_intent),
                ("expected answer", &contract.expected),
            ] {
                require_safe_text(field, value)?;
            }
            if !questions.insert(contract.question.to_ascii_lowercase()) {
                bail!("duplicate anticipated question {:?}", contract.question);
            }
            let query = contract.sparql.to_ascii_uppercase();
            if !(query.split_whitespace().any(|word| word == "SELECT")
                || query.split_whitespace().any(|word| word == "ASK"))
            {
                bail!("anticipated question SPARQL must contain SELECT or ASK");
            }
            require_safe_text("anticipated question SPARQL", &contract.sparql)?;
        }
        Ok(())
    }
}

fn require_safe_text(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    let lower = value.to_ascii_lowercase();
    for marker in [
        "password=",
        "token=",
        "secret=",
        "authorization:",
        "bearer ",
    ] {
        if lower.contains(marker) {
            bail!("{field} appears to contain a credential ({marker})");
        }
    }
    Ok(())
}

impl Plan {
    pub fn for_profile(profile: Profile) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profile,
            tools: profile.tools(),
            shares: Vec::new(),
            quipu_db: None,
            quipu_flavor: QuipuFlavor::default(),
            crew: None,
            intent: None,
        }
    }

    pub fn for_crew(mode: CrewMode) -> Self {
        Self {
            profile: Profile::Crew,
            crew: Some(CrewSelection::for_mode(mode)),
            ..Self::for_profile(Profile::Crew)
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let body =
            fs::read_to_string(path).with_context(|| format!("read plan {}", path.display()))?;
        let plan: Self =
            toml::from_str(&body).with_context(|| format!("parse plan {}", path.display()))?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let body = toml::to_string_pretty(self).context("serialize plan")?;
        fs::write(path, body).with_context(|| format!("write plan {}", path.display()))
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            bail!(
                "unsupported plan schema {}; expected {}",
                self.schema_version,
                SCHEMA_VERSION
            );
        }
        if self.tools.is_empty() {
            bail!("plan selects no tools");
        }
        if self.tools != self.profile.tools() {
            bail!(
                "tools do not match the {:?} profile conventions",
                self.profile
            );
        }
        if !self.shares.is_empty() && self.quipu_db.is_none() {
            bail!("plans with knowledge shares require an explicit --quipu-db");
        }
        if self.shares.iter().any(|path| path.as_os_str().is_empty()) {
            bail!("knowledge share paths must not be empty");
        }
        if let Some(intent) = &self.intent {
            intent.validate()?;
        }
        match (self.profile, &self.crew) {
            (Profile::Crew, Some(crew)) => crew.validate()?,
            (Profile::Crew, None) => bail!("crew profile requires a [crew] selection"),
            (Profile::Everything, Some(crew)) => crew.validate()?,
            (_, Some(_)) => bail!("[crew] is only valid with profile = \"crew\" or \"everything\""),
            (_, None) => {}
        }
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub schema_version: u32,
    pub tools: BTreeMap<String, ToolState>,
    #[serde(default)]
    pub crew: BTreeMap<String, CrewRuntimeState>,
    #[serde(default)]
    pub shares: BTreeMap<String, ShareState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolState {
    pub version: String,
    pub applied: bool,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewRuntimeState {
    pub version: String,
    pub applied: bool,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareState {
    pub path: PathBuf,
    pub staging_graph: String,
    pub outcome: String,
    pub promotion_eligible: bool,
    pub blockers: Vec<String>,
}

impl State {
    pub fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                schema_version: SCHEMA_VERSION,
                tools: BTreeMap::new(),
                crew: BTreeMap::new(),
                shares: BTreeMap::new(),
            });
        }
        let body =
            fs::read_to_string(path).with_context(|| format!("read state {}", path.display()))?;
        let state: Self = serde_json::from_str(&body)
            .with_context(|| format!("parse state {}", path.display()))?;
        if state.schema_version != SCHEMA_VERSION {
            bail!("unsupported state schema {}", state.schema_version);
        }
        Ok(state)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("create state directory {}", parent.display()))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("create temporary state in {}", parent.display()))?;
        serde_json::to_writer_pretty(&mut tmp, self).context("serialize state")?;
        use std::io::Write;
        writeln!(tmp).context("finish state")?;
        tmp.persist(path)
            .map_err(|error| error.error)
            .with_context(|| format!("replace state {}", path.display()))?;
        Ok(())
    }
}

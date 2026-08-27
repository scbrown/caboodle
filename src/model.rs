use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Kg,
    Retrieval,
}

impl Profile {
    pub fn tools(self) -> Vec<ToolName> {
        match self {
            Self::Kg => vec![ToolName::Quipu],
            Self::Retrieval => vec![ToolName::Quipu, ToolName::Bobbin],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolName {
    Quipu,
    Bobbin,
}

impl ToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quipu => "quipu",
            Self::Bobbin => "bobbin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub profile: Profile,
    pub tools: Vec<ToolName>,
}

impl Plan {
    pub fn for_profile(profile: Profile) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profile,
            tools: profile.tools(),
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
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub schema_version: u32,
    pub tools: BTreeMap<String, ToolState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolState {
    pub version: String,
    pub applied: bool,
    pub verified: bool,
}

impl State {
    pub fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                schema_version: SCHEMA_VERSION,
                tools: BTreeMap::new(),
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

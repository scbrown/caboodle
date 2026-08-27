use std::{fs, path::Path};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::model::{CrewMode, CrewOwner, CrewPolicy, CrewRouting, Plan};

#[derive(Serialize)]
struct SharedPolicy<'a> {
    identity_source: crate::model::IdentitySource,
    model: &'a Option<String>,
    tools: Vec<&'static str>,
}

impl<'a> From<&'a CrewPolicy> for SharedPolicy<'a> {
    fn from(policy: &'a CrewPolicy) -> Self {
        Self {
            identity_source: policy.identity_source,
            model: &policy.model,
            tools: policy.tools.iter().map(|tool| tool.as_str()).collect(),
        }
    }
}

#[derive(Serialize)]
struct ShantytownProjection<'a> {
    schema: &'static str,
    shared: SharedPolicy<'a>,
    settings_owner: &'static str,
    durable_owner: CrewOwner,
    routing: Option<CrewRouting>,
    hooks: &'static str,
    filesystem: &'static str,
}

#[derive(Serialize)]
struct CreelProjection<'a> {
    schema: &'static str,
    shared: SharedPolicy<'a>,
    settings_owner: &'static str,
    burst_owner: CrewOwner,
    routing: Option<CrewRouting>,
    credential_policy: &'static str,
    browser_permissions: &'static str,
}

pub fn write(plan: &Plan, output: &Path) -> Result<Vec<String>> {
    plan.validate()?;
    let crew = plan
        .crew
        .as_ref()
        .context("settings projection requires a crew profile")?;
    fs::create_dir_all(output)
        .with_context(|| format!("create projection directory {}", output.display()))?;
    let mut written = Vec::new();

    if matches!(crew.mode, CrewMode::Shantytown | CrewMode::Both) {
        let projection = ShantytownProjection {
            schema: "caboodle.shantytown-settings/v1",
            shared: (&crew.policy).into(),
            settings_owner: "shantytown",
            durable_owner: crew.durable_owner.context("missing durable owner")?,
            routing: crew.routing,
            hooks: "adapter-emitted",
            filesystem: "host-workspace",
        };
        write_json(output, "shantytown.settings.json", &projection)?;
        written.push("shantytown.settings.json".to_owned());
    }

    if matches!(crew.mode, CrewMode::Creel | CrewMode::Both) {
        let projection = CreelProjection {
            schema: "caboodle.creel-settings/v1",
            shared: (&crew.policy).into(),
            settings_owner: "creel",
            burst_owner: crew.burst_owner.context("missing burst owner")?,
            routing: crew.routing,
            credential_policy: "browser-byo-write-only",
            browser_permissions: "operator-granted",
        };
        write_json(output, "creel.settings.json", &projection)?;
        written.push("creel.settings.json".to_owned());
    }

    if crew.mode == CrewMode::Standalone {
        bail!("standalone crew mode has no harness settings to project");
    }
    Ok(written)
}

fn write_json<T: Serialize>(output: &Path, name: &str, value: &T) -> Result<()> {
    let path = output.join(name);
    let body = serde_json::to_vec_pretty(value).context("serialize settings projection")?;
    fs::write(&path, body).with_context(|| format!("write projection {}", path.display()))
}

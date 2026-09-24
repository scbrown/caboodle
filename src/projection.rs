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

/// Delegate both crew discovery and registration to the authority that owns them.
/// Reading plan.crew here would fork the rig's roster and miss non-crew installs.
pub fn register(root: Option<&Path>, agent: Option<&str>, registry: &str) -> Result<()> {
    use std::process::{Command, Stdio};
    let mut command = Command::new("st");
    if let Some(root) = root {
        command.arg("--root").arg(root);
    }
    command.args(["--registry", registry, "ops", "provision", "--json"]);
    if let Some(agent) = agent {
        command.arg("--").arg(agent);
    }
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .context("run shantytown registration; install st with ops provision support first")?;
    if !output.status.success() {
        bail!("shantytown registration failed ({}); inspect its refusal above; no registration success claimed", output.status);
    }
    // A successful process is not a registration proof: demand the owner's
    // read-back receipt, and never echo a malformed response that could contain secrets.
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("shantytown returned no valid registration receipt")?;
    let agents = receipt
        .get("agents")
        .and_then(|v| v.as_array())
        .context("shantytown registration receipt has no agents")?;
    if receipt["version"] != 1 || receipt["owner"] != "shantytown" || agents.is_empty() {
        bail!("shantytown registration receipt has an unsupported version, owner, or empty crew");
    }
    let mut verified = Vec::new();
    for row in agents {
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .context("registration receipt has no agent name")?;
        let servers = row
            .get("servers")
            .and_then(|v| v.as_array())
            .filter(|v| !v.is_empty())
            .context("registration receipt has no MCP servers")?;
        if servers
            .iter()
            .any(|v| v.as_str().map_or(true, str::is_empty))
        {
            bail!("registration receipt contains an invalid MCP server name");
        }
        verified.push((name, servers.len()));
    }
    for (name, count) in verified {
        println!("registered: {name} ({count} MCP servers; owner: shantytown)");
    }
    println!("Existing sessions load registration on their next start.");
    Ok(())
}

//! Credential proof for the live client, separate from isolated database tests.
use std::{env, fs, path::Path, process::Command};

use anyhow::{bail, Context, Result};

const FIX: &str = "install the issued token with `install -d -m 700 \"$HOME/.config/quipu\" && install -m 400 /secure/issued-token \"$HOME/.config/quipu/token\"`; run `caboodle doctor`";

fn check_file(home: &Path) -> Result<()> {
    let path = home.join(".config/quipu/token");
    let metadata = fs::metadata(&path).with_context(|| {
        format!("missing canonical Quipu credential ~/.config/quipu/token; {FIX}")
    })?;
    if !metadata.is_file() {
        bail!("canonical Quipu credential is not a regular file; {FIX}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = fs::metadata(path.parent().expect("token has parent"))?;
        if metadata.permissions().mode() & 0o777 != 0o400
            || parent.permissions().mode() & 0o777 != 0o700
        {
            bail!("Quipu credential requires file mode 0400 and parent mode 0700; {FIX}");
        }
    }
    let value = fs::read_to_string(path)
        .with_context(|| format!("canonical Quipu credential is unreadable or not UTF-8; {FIX}"))?;
    if value.trim().is_empty() {
        bail!("canonical Quipu credential is empty; {FIX}");
    }
    Ok(())
}

fn classify(code: u16) -> Result<()> {
    match code {
        200..=299 => Ok(()),
        401 => bail!("Quipu credential rejected by authenticated /shapes read; checked QUIPU_AUTH_TOKEN > QUIPU_AUTH_TOKEN_FILE > ~/.config/quipu/token; verify issuer and server; {FIX}"),
        other => bail!("Quipu credential acceptance unproven: /shapes HTTP {other}; check server/connectivity; run `caboodle doctor`"),
    }
}

/// Prove the canonical file and the resolver-selected credential on this host.
/// `/shapes` is an authenticated read; public health/query cannot prove auth.
pub(crate) fn verify(server: &str) -> Result<()> {
    let home =
        env::var_os("HOME").context("HOME missing; cannot check canonical Quipu credential")?;
    check_file(Path::new(&home))?;
    let auth = crate::quipu_auth::config()?.with_context(|| format!(
        "Quipu credential missing; checked QUIPU_AUTH_TOKEN > QUIPU_AUTH_TOKEN_FILE > ~/.config/quipu/token; {FIX}"
    ))?;
    let control = read_status(server, None, "auth-negative-probe")?;
    if control != 401 {
        bail!("Quipu credential acceptance unproven: unauthenticated /shapes control returned HTTP {control}, expected 401; run `caboodle doctor`");
    }
    classify(read_status(server, Some(auth.path()), "caboodle-verify")?)
}

/// Install an issued credential after proving it, without replacing an identity.
pub(crate) fn provision(source: &Path, server: &str, home: &Path) -> Result<()> {
    use std::io::Write;
    let value = fs::read_to_string(source).context("cannot read issued Quipu credential file")?;
    let value = value.trim();
    if value.is_empty() {
        bail!("issued Quipu credential is empty; {FIX}");
    }
    let mut auth =
        tempfile::NamedTempFile::new().context("create private credential probe config")?;
    auth.write_all(crate::adapter::curl_auth_header_line(value)?.as_bytes())?;
    if read_status(server, None, "auth-negative-probe")? != 401 {
        bail!(
            "Quipu credential acceptance unproven: unauthenticated /shapes control must return 401"
        );
    }
    classify(read_status(server, Some(auth.path()), "caboodle-verify")?)?;
    let path = home.join(".config/quipu/token");
    match fs::read_to_string(&path) {
        Ok(existing) if existing.trim() != value => bail!("refusing to replace an existing Quipu credential; rotation requires a separate explicit decision"),
        Ok(_) => return check_file(home),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(_) => bail!("cannot inspect existing Quipu credential; refusing to replace it"),
    }
    let parent = path.parent().expect("token has parent");
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let mut candidate =
            tempfile::NamedTempFile::new_in(parent).context("stage canonical Quipu credential")?;
        candidate.write_all(value.as_bytes())?;
        candidate.as_file().sync_all()?;
        candidate
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o400))?;
        candidate.persist_noclobber(&path).map_err(|_| {
            anyhow::anyhow!(
                "canonical credential appeared during provisioning; refusing to replace it"
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        bail!(
            "cannot prove required 0400/0700 permissions on this platform; no credential installed"
        );
    }
    check_file(home)
}

fn read_status(server: &str, auth: Option<&Path>, client: &str) -> Result<u16> {
    let mut command = Command::new("curl");
    command
        .args([
            "--disable",
            "--silent",
            "--max-time",
            "5",
            "--request",
            "POST",
            "--header",
            "Content-Type: application/json",
            "--header",
        ])
        .arg(format!("X-Quipu-Client: {client}"))
        .args([
            "--data",
            "{\"action\":\"list\"}",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
        ]);
    if let Some(path) = auth {
        command.arg("--config").arg(path);
    }
    let output = command
        .arg(format!("{}/shapes", server.trim_end_matches('/')))
        .output()
        .context("cannot run authenticated Quipu credential read")?;
    if !output.status.success() {
        bail!("Quipu credential acceptance unproven: authenticated read transport failed; check server/connectivity; run `caboodle doctor`");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_status_never_confuses_public_health_or_refusal_with_acceptance() {
        assert!(classify(200).is_ok());
        assert!(classify(204).is_ok());
        for code in [0, 301, 400, 401, 403, 500] {
            assert!(classify(code).is_err());
        }
        assert!(classify(401).unwrap_err().to_string().contains("rejected"));
    }
    #[test]
    #[cfg(unix)]
    fn canonical_file_requires_content_and_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        assert!(check_file(home.path()).is_err());
        let parent = home.path().join(".config/quipu");
        fs::create_dir_all(&parent).unwrap();
        let token = parent.join("token");
        fs::write(&token, "fixture-not-a-live-credential").unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&token, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(check_file(home.path()).is_ok());
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(check_file(home.path()).is_err());
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(check_file(home.path()).is_err());
        fs::remove_file(&token).unwrap();
        fs::write(&token, "\n").unwrap();
        fs::set_permissions(&token, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(check_file(home.path()).is_err());
    }
}

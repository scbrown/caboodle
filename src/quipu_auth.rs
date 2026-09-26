//! Request-time credential discovery shared by delivery and diagnostics.
use std::{env, fs, path::Path};

use anyhow::{Context, Result};

pub(crate) fn token() -> Result<Option<String>> {
    resolve(
        env::var("QUIPU_AUTH_TOKEN").ok().as_deref(),
        env::var_os("QUIPU_AUTH_TOKEN_FILE")
            .as_deref()
            .map(Path::new),
        env::var_os("HOME").as_deref().map(Path::new),
    )
}

fn resolve(
    value: Option<&str>,
    explicit: Option<&Path>,
    home: Option<&Path>,
) -> Result<Option<String>> {
    if let Some(value) = value.filter(|s| !s.is_empty()) {
        return Ok(Some(value.to_owned()));
    }
    let default = home.map(|p| p.join(".config/quipu/token"));
    let Some(path) = explicit
        .filter(|p| !p.as_os_str().is_empty())
        .or(default.as_deref())
    else {
        return Ok(None);
    };
    match fs::read_to_string(path) {
        Ok(value) => Ok((!value.trim().is_empty()).then(|| value.trim().to_owned())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => anyhow::bail!("cannot read Quipu token file; check QUIPU_AUTH_TOKEN_FILE or ~/.config/quipu/token permissions and UTF-8 encoding"),
    }
}

pub(crate) fn config() -> Result<Option<tempfile::NamedTempFile>> {
    let Some(token) = token()? else {
        return Ok(None);
    };
    let file = tempfile::NamedTempFile::new().context("create temporary Quipu auth config")?;
    fs::write(file.path(), crate::adapter::curl_auth_header_line(&token)?)
        .context("write temporary Quipu auth config")?;
    Ok(Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_rotation_missing_and_empty_are_isolated() {
        let home = tempfile::tempdir().unwrap();
        let default = home.path().join(".config/quipu/token");
        fs::create_dir_all(default.parent().unwrap()).unwrap();
        fs::write(&default, "default\n").unwrap();
        let explicit = home.path().join("explicit");
        assert_eq!(
            resolve(None, None, Some(home.path())).unwrap().as_deref(),
            Some("default")
        );
        assert_eq!(
            resolve(None, Some(&explicit), Some(home.path())).unwrap(),
            None
        );
        fs::write(&explicit, "first\n").unwrap();
        assert_eq!(
            resolve(None, Some(&explicit), Some(home.path()))
                .unwrap()
                .as_deref(),
            Some("first")
        );
        fs::write(&explicit, "second\n").unwrap();
        assert_eq!(
            resolve(Some(""), Some(&explicit), Some(home.path()))
                .unwrap()
                .as_deref(),
            Some("second")
        );
        assert_eq!(
            resolve(Some("override"), Some(&explicit), Some(home.path()))
                .unwrap()
                .as_deref(),
            Some("override")
        );
        fs::write(&explicit, "\n").unwrap();
        assert_eq!(
            resolve(None, Some(&explicit), Some(home.path())).unwrap(),
            None
        );
        fs::write(&explicit, [0xff]).unwrap();
        assert!(resolve(None, Some(&explicit), Some(home.path())).is_err());
    }
}

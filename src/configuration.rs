use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::model::StackConfig;

pub fn default_path() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is unset; cannot locate global stack config")?;
    Ok(PathBuf::from(home).join(".config/bobbin/config.toml"))
}

pub fn apply(config: &StackConfig) -> Result<()> {
    apply_to(config, &default_path()?)
}

pub fn apply_to(config: &StackConfig, path: &Path) -> Result<()> {
    let mut document = if path.exists() {
        let body = fs::read_to_string(path)
            .with_context(|| format!("read stack config {}", path.display()))?;
        body.parse::<DocumentMut>().with_context(|| {
            format!(
                "parse stack config {}; refusing to overwrite it",
                path.display()
            )
        })?
    } else {
        DocumentMut::new()
    };
    if document.get("quipu").is_none() {
        document["quipu"] = Item::Table(Table::new());
    }
    document["quipu"]
        .as_table_mut()
        .context("[quipu] must be a TOML table")?;
    if document["quipu"].get("owl").is_none() {
        document["quipu"]["owl"] = Item::Table(Table::new());
    }
    document["quipu"]["owl"]
        .as_table_mut()
        .context("[quipu.owl] must be a TOML table")?;
    document["quipu"]["owl"]["reactive_materialize"] = value(config.quipu.owl.reactive_materialize);

    let rendered = document.to_string();
    let parent = path.parent().context("stack config path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create stack config directory {}", parent.display()))?;
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, rendered).with_context(|| format!("write {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("atomically replace {}", path.display()))?;

    let read_back: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    if read_back["quipu"]["owl"]["reactive_materialize"].as_bool()
        != Some(config.quipu.owl.reactive_materialize)
    {
        bail!("stack config read-back did not preserve quipu.owl.reactive_materialize");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::apply_to;
    use crate::model::StackConfig;

    #[test]
    fn recommended_config_enables_reactive_owl_and_preserves_other_values() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(
            &path,
            "# keep this comment\n[search]\nlimit = 17\n\n[quipu.owl]\nvalidate_on_write = false\n",
        )
        .unwrap();

        apply_to(&StackConfig::recommended(), &path).unwrap();

        let value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["search"]["limit"].as_integer(), Some(17));
        assert_eq!(
            value["quipu"]["owl"]["validate_on_write"].as_bool(),
            Some(false)
        );
        assert_eq!(
            value["quipu"]["owl"]["reactive_materialize"].as_bool(),
            Some(true)
        );
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("# keep this comment"));
    }
}

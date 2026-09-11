use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub github_repo: Option<String>,
    #[serde(default)]
    pub egress: Egress,
    #[serde(default = "Resources::default")]
    pub resources: Resources,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Egress {
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resources {
    pub cpu: String,
    pub mem: String,
    pub pids: u32,
}

impl Default for Resources {
    fn default() -> Self {
        Resources {
            cpu: "2.0".to_string(),
            mem: "4g".to_string(),
            pids: 512,
        }
    }
}

impl Manifest {
    pub fn new(name: &str, image: &str) -> Self {
        Manifest {
            name: name.to_string(),
            image: image.to_string(),
            github_repo: None,
            egress: Egress::default(),
            resources: Resources::default(),
            secrets: vec!["ANTHROPIC_API_KEY".to_string(), "GITHUB_TOKEN".to_string()],
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parsing manifest {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_yaml::to_string(self)?;
        std::fs::write(path, text)
            .with_context(|| format!("writing manifest {}", path.display()))?;
        Ok(())
    }

    pub fn sandbox_container(&self) -> String {
        format!("{}-sandbox", self.name)
    }

    pub fn egress_container(&self) -> String {
        format!("{}-egress", self.name)
    }
}

/// Validate a project name: lowercase alnum + dashes, matches what's safe
/// to use as a Docker container/network/volume name and a directory name.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && name.chars().next().map_or(false, |c| c.is_ascii_alphabetic());
    if !ok {
        anyhow::bail!(
            "invalid project name '{name}': use lowercase letters, digits, and dashes, starting with a letter"
        );
    }
    Ok(())
}

use crate::manifest::Manifest;

const TEMPLATE: &str = include_str!("../../compose/project.compose.yml.tmpl");

/// Render the per-project docker-compose file from the manifest. Pure
/// string substitution — no templating engine — so `compose/project.compose.yml.tmpl`
/// stays the readable ground truth for exactly what gets applied.
pub fn render(m: &Manifest) -> String {
    let secret_env_lines = if m.secrets.is_empty() {
        String::new()
    } else {
        let mut lines = String::new();
        for s in &m.secrets {
            lines.push_str(&format!("      {s}: \"${{{s}:-}}\"\n"));
        }
        // drop the trailing newline; the template line already provides one
        lines.trim_end_matches('\n').to_string()
    };

    let extra_allow = m.egress.allow.join(",");

    TEMPLATE
        .replace("{{PROJECT_NAME}}", &m.name)
        .replace("{{IMAGE}}", &m.image)
        .replace("{{CPU_LIMIT}}", &m.resources.cpu)
        .replace("{{MEM_LIMIT}}", &m.resources.mem)
        .replace("{{PIDS_LIMIT}}", &m.resources.pids.to_string())
        .replace("{{SECRET_ENV_LINES}}", &secret_env_lines)
        .replace("{{EXTRA_ALLOW_DOMAINS}}", &extra_allow)
}

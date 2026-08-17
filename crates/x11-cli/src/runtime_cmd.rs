use anyhow::Result;
use std::{env, path::PathBuf};
use crate::runtime::{self, RuntimeKind, Source};

pub fn print_status(json: bool) -> Result<()> {
    let workspace = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let rows = runtime::inspect(&workspace);
    if json {
        let rows = rows.into_iter().map(|r| serde_json::json!({
            "runtime": match r.kind { RuntimeKind::Node => "node", RuntimeKind::Python => "python" },
            "source": match r.source { Source::System => "system", Source::Managed => "managed", Source::Missing => "missing" },
            "executable": r.executable.map(|p| p.display().to_string()),
            "version": r.version,
            "requested": r.requested,
            "reason": r.project_reason,
        })).collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    println!("X11 Runtime\n");
    if rows.is_empty() {
        println!("No Node.js or Python runtime is required by this workspace.");
        return Ok(());
    }
    for r in rows {
        let kind = match r.kind { RuntimeKind::Node => "Node.js", RuntimeKind::Python => "Python" };
        let source = match r.source { Source::System => "system", Source::Managed => "managed", Source::Missing => "missing" };
        let version = r.version.unwrap_or_else(|| "unknown".into());
        let requested = r.requested.map(|v| format!(" (requested {v})")).unwrap_or_default();
        println!("{kind:<10} {source:<7} {version}{requested}");
        if let Some(path) = r.executable { println!("  path: {}", path.display()); }
        if let Some(reason) = r.project_reason { println!("  detected from: {reason}"); }
    }
    Ok(())
}

pub async fn install(runtime_name: &str, version: &str) -> Result<()> {
    match runtime_name.to_ascii_lowercase().as_str() {
        "node" | "nodejs" => runtime::install_node(version).await,
        "python" | "python3" => crate::python_runtime::install(version).await,
        other => anyhow::bail!("unknown runtime '{other}'; supported installers: node, python"),
    }
}

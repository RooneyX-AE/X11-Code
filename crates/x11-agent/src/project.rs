use anyhow::Result;
use std::{fs, path::{Path, PathBuf}};
use x11_core::orchestration::Skill;

#[derive(Debug, Clone, Default)]
pub struct ProjectInstructions {
    pub files: Vec<PathBuf>,
    pub text: String,
}

pub fn load_agents_md(workspace: &Path) -> Result<ProjectInstructions> {
    let mut roots = Vec::new();
    let mut cursor = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    loop {
        roots.push(cursor.clone());
        if !cursor.pop() { break; }
    }
    roots.reverse();
    let mut files = Vec::new();
    let mut sections = Vec::new();
    for root in roots {
        let path = root.join("AGENTS.md");
        if path.is_file() {
            sections.push(format!("## {}\n{}", path.display(), fs::read_to_string(&path)?));
            files.push(path);
        }
    }
    Ok(ProjectInstructions { files, text: sections.join("\n\n") })
}

pub fn discover_skills(workspace: &Path) -> Result<Vec<Skill>> {
    let mut candidates = Vec::new();
    let mut roots = Vec::new();
    let mut cursor = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    loop {
        roots.push(cursor.clone());
        if !cursor.pop() { break; }
    }
    roots.reverse();
    for root in roots {
        for dir in [root.join(".agents/skills"), root.join(".x11/skills")] {
            if !dir.is_dir() { continue; }
            scan_skill_dir(&dir, &mut candidates)?;
        }
    }
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates.dedup_by(|a, b| a.name == b.name);
    Ok(candidates)
}

fn scan_skill_dir(dir: &Path, out: &mut Vec<Skill>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                out.push(parse_skill(&skill_file)?);
            }
        } else if path.file_name().and_then(|v| v.to_str()) == Some("SKILL.md") {
            out.push(parse_skill(&path)?);
        }
    }
    Ok(())
}

fn parse_skill(path: &Path) -> Result<Skill> {
    let text = fs::read_to_string(path)?;
    let mut name = path.file_stem().and_then(|v| v.to_str()).unwrap_or("skill").to_owned();
    let mut description = String::new();
    let mut body_start = 0usize;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.first().map(|v| v.trim()) == Some("---") {
        for (idx, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" { body_start = idx + 1; break; }
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let value = v.trim().trim_matches(['"', '\'']);
                match key { "name" if !value.is_empty() => name = value.to_owned(), "description" => description = value.to_owned(), _ => {} }
            }
        }
    }
    let instructions = lines.get(body_start..).unwrap_or(&[]).join("\n").trim().to_owned();
    Ok(Skill { name, description, instructions, tool_hints: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_skill_frontmatter() {
        let base = std::env::temp_dir().join(format!("x11-skill-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let dir = base.join(".agents/skills/review");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: review-code\ndescription: Review source\n---\nInspect the diff and report regressions.").unwrap();
        let skills = discover_skills(&base).unwrap();
        assert_eq!(skills[0].name, "review-code");
        assert_eq!(skills[0].description, "Review source");
        assert!(skills[0].instructions.contains("regressions"));
        fs::remove_dir_all(base).unwrap();
    }
}

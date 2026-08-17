use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::HashMap, path::{Component, Path, PathBuf}, sync::Arc, time::Duration};
use tokio::{process::Command, time::timeout};

const MAX_OUTPUT: usize = 48_000;
const MAX_FILE_WRITE: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ToolContext { pub workspace: PathBuf }

impl ToolContext {
    pub async fn root(&self) -> Result<PathBuf> { Ok(tokio::fs::canonicalize(&self.workspace).await?) }

    pub async fn path(&self, p: &str, write: bool) -> Result<PathBuf> {
        let rel = Path::new(p);
        if rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir)) {
            anyhow::bail!("path escapes workspace")
        }
        let root = self.root().await?;
        let candidate = root.join(rel);

        if write {
            if tokio::fs::try_exists(&candidate).await? {
                let canon = tokio::fs::canonicalize(&candidate).await?;
                if !canon.starts_with(&root) { anyhow::bail!("path escapes workspace") }
                if canon.is_dir() { anyhow::bail!("target is a directory") }
            }
            if let Some(parent) = candidate.parent() {
                let canon_parent = if tokio::fs::try_exists(parent).await? {
                    tokio::fs::canonicalize(parent).await?
                } else {
                    parent.to_path_buf()
                };
                if !canon_parent.starts_with(&root) { anyhow::bail!("path escapes workspace") }
            }
            Ok(candidate)
        } else {
            let canon = tokio::fs::canonicalize(candidate).await?;
            if !canon.starts_with(&root) { anyhow::bail!("path escapes workspace") }
            Ok(canon)
        }
    }
}

fn truncate(s: impl Into<String>) -> String {
    let mut s = s.into();
    if s.len() <= MAX_OUTPUT { return s; }
    s.truncate(MAX_OUTPUT);
    s.push_str("\n...[output truncated by X11 Code]...");
    s
}

async fn atomic_replace(path: &Path, content: &str) -> Result<()> {
    if content.len() > MAX_FILE_WRITE { anyhow::bail!("file content exceeds 8 MiB limit") }
    let parent = path.parent().context("target has no parent")?;
    let tmp = parent.join(format!(".x11-tmp-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&tmp, content).await?;
    if let Err(error) = tokio::fs::rename(&tmp, path).await {
        #[cfg(windows)] {
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                tokio::fs::remove_file(path).await?;
                tokio::fs::rename(&tmp, path).await?;
                return Ok(());
            }
        }
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(error.into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind { ReadOnly, FilesystemWrite, Shell, GitWrite, Network }

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn kind(&self) -> ToolKind;
    fn input_schema(&self) -> Value;
    async fn execute(&self, ctx: &ToolContext, input: Value) -> Result<String>;
}

#[derive(Clone, Default)]
pub struct ToolRegistry { tools: HashMap<String, Arc<dyn Tool>> }
impl ToolRegistry {
    pub fn register<T: Tool + 'static>(&mut self, tool: T) { self.tools.insert(tool.name().to_owned(), Arc::new(tool)); }
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> { self.tools.get(name).cloned() }
    pub fn definitions(&self) -> Vec<Value> {
        let mut v = self.tools.values().map(|t| json!({"name":t.name(),"description":t.description(),"input_schema":t.input_schema()})).collect::<Vec<_>>();
        v.sort_by(|a,b|a["name"].as_str().cmp(&b["name"].as_str())); v
    }
    pub async fn execute(&self, c: &ToolContext, name: &str, input: Value) -> Result<String> { self.get(name).context("unknown tool")?.execute(c,input).await }
    pub fn builtins() -> Self {
        let mut r = Self::default();
        r.register(ReadFile); r.register(WriteFile); r.register(EditFile); r.register(ListFiles);
        r.register(Shell); r.register(Search); r.register(Git); r.register(GitStatus); r.register(GitDiff); r
    }
}

pub struct ReadFile;
#[async_trait] impl Tool for ReadFile {
    fn name(&self)->&str{"read_file"}
    fn description(&self)->&str{"Read a UTF-8 file inside the workspace."}
    fn kind(&self)->ToolKind{ToolKind::ReadOnly}
    fn input_schema(&self)->Value{json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})}
    async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let p=i["path"].as_str().context("path")?;Ok(truncate(tokio::fs::read_to_string(c.path(p,false).await?).await?))}
}

pub struct WriteFile;
#[async_trait] impl Tool for WriteFile {
    fn name(&self)->&str{"write_file"}
    fn description(&self)->&str{"Write or replace a UTF-8 file inside the workspace."}
    fn kind(&self)->ToolKind{ToolKind::FilesystemWrite}
    fn input_schema(&self)->Value{json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false})}
    async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let p=i["path"].as_str().context("path")?;let content=i["content"].as_str().context("content")?;let t=c.path(p,true).await?;if let Some(parent)=t.parent(){tokio::fs::create_dir_all(parent).await?;}atomic_replace(&t,content).await?;Ok(format!("wrote {} bytes to {p}",content.len()))}
}

pub struct EditFile;
#[async_trait] impl Tool for EditFile {
    fn name(&self)->&str{"edit_file"}
    fn description(&self)->&str{"Replace exactly one text occurrence in a workspace file."}
    fn kind(&self)->ToolKind{ToolKind::FilesystemWrite}
    fn input_schema(&self)->Value{json!({"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"],"additionalProperties":false})}
    async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let p=i["path"].as_str().context("path")?;let old=i["old"].as_str().context("old")?;let new=i["new"].as_str().context("new")?;if old.is_empty(){anyhow::bail!("old text cannot be empty")}if new.len()>MAX_FILE_WRITE{anyhow::bail!("new text exceeds 8 MiB limit")}let t=c.path(p,false).await?;let s=tokio::fs::read_to_string(&t).await?;let n=s.matches(old).count();if n!=1{anyhow::bail!("expected one match, found {n}")}let updated=s.replacen(old,new,1);atomic_replace(&t,&updated).await?;Ok(format!("edited {p}"))}
}

pub struct ListFiles;
#[async_trait] impl Tool for ListFiles {fn name(&self)->&str{"list_files"} fn description(&self)->&str{"List files and directories below a workspace path."} fn kind(&self)->ToolKind{ToolKind::ReadOnly} fn input_schema(&self)->Value{json!({"type":"object","properties":{"path":{"type":"string"},"depth":{"type":"integer","minimum":0,"maximum":8}},"required":[],"additionalProperties":false})} async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let p=i["path"].as_str().unwrap_or(".");let depth=i["depth"].as_u64().unwrap_or(2).min(8)as usize;let root=c.path(p,false).await?;let base=c.root().await?;let mut out=Vec::new();let mut stack=vec![(root,0usize)];while let Some((dir,d))=stack.pop(){if d>depth{continue}let mut rd=tokio::fs::read_dir(&dir).await?;while let Some(e)=rd.next_entry().await?{let path=e.path();if path.file_name().and_then(|x|x.to_str())==Some(".git"){continue}let rel=path.strip_prefix(&base).unwrap_or(&path).display().to_string();out.push(rel);if d<depth&&e.file_type().await?.is_dir(){stack.push((path,d+1));}}}out.sort();Ok(truncate(out.join("\n")))}}

pub struct Shell;
#[async_trait] impl Tool for Shell {fn name(&self)->&str{"shell"} fn description(&self)->&str{"Run a bounded shell command in the workspace."} fn kind(&self)->ToolKind{ToolKind::Shell} fn input_schema(&self)->Value{json!({"type":"object","properties":{"command":{"type":"string"},"timeout_ms":{"type":"integer","minimum":100,"maximum":120000}},"required":["command"],"additionalProperties":false})} async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let cmd=i["command"].as_str().context("command")?;if cmd.len()>16000{anyhow::bail!("command too long")}let ms=i["timeout_ms"].as_u64().unwrap_or(30000).clamp(100,120000);let mut q=Command::new(if cfg!(windows){"cmd"}else{"sh"});q.args(if cfg!(windows){vec!["/C",cmd]}else{vec!["-lc",cmd]}).current_dir(&c.workspace);let o=timeout(Duration::from_millis(ms),q.output()).await.context("command timed out")??;Ok(truncate(format!("exit={}\nstdout:\n{}\nstderr:\n{}",o.status.code().unwrap_or(-1),String::from_utf8_lossy(&o.stdout),String::from_utf8_lossy(&o.stderr))))}}

pub struct Search;
#[async_trait] impl Tool for Search {fn name(&self)->&str{"search"} fn description(&self)->&str{"Search text recursively, using ripgrep when available."} fn kind(&self)->ToolKind{ToolKind::ReadOnly} fn input_schema(&self)->Value{json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"],"additionalProperties":false})} async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let pattern=i["pattern"].as_str().context("pattern")?;if pattern.is_empty(){anyhow::bail!("pattern cannot be empty")}let path=i["path"].as_str().unwrap_or(".");let target=c.path(path,false).await?;let rg=Command::new("rg").args(["--line-number","--hidden","--glob","!.git/*",pattern,target.to_str().unwrap_or(".")]).current_dir(&c.workspace).output().await;if let Ok(o)=rg{if o.status.success()||o.status.code()==Some(1){return Ok(truncate(String::from_utf8_lossy(&o.stdout).into_owned()))}}let mut hits=Vec::new();let mut stack=vec![target];while let Some(dir)=stack.pop(){let mut rd=match tokio::fs::read_dir(&dir).await{Ok(v)=>v,Err(_)=>continue};while let Some(e)=rd.next_entry().await?{let p=e.path();if p.file_name().and_then(|s|s.to_str())==Some(".git"){continue}if e.file_type().await?.is_dir(){stack.push(p)}else if let Ok(text)=tokio::fs::read_to_string(&p).await{for(i,l)in text.lines().enumerate(){if l.contains(pattern){hits.push(format!("{}:{}:{}",p.strip_prefix(&c.workspace).unwrap_or(&p).display(),i+1,l));}}}}}Ok(truncate(hits.join("\n")))}}

pub struct Git;
#[async_trait] impl Tool for Git {fn name(&self)->&str{"git"}fn description(&self)->&str{"Run git with an explicit argument array."}fn kind(&self)->ToolKind{ToolKind::GitWrite}fn input_schema(&self)->Value{json!({"type":"object","properties":{"args":{"type":"array","items":{"type":"string"}}},"required":["args"],"additionalProperties":false})}async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let a=i["args"].as_array().context("args")?.iter().map(|v|v.as_str().map(str::to_owned).context("git arg must be string")).collect::<Result<Vec<_>>>()?;if a.len()>64{anyhow::bail!("too many git arguments")}let o=Command::new("git").args(a).current_dir(&c.workspace).output().await?;Ok(truncate(format!("exit={}\nstdout:\n{}\nstderr:\n{}",o.status.code().unwrap_or(-1),String::from_utf8_lossy(&o.stdout),String::from_utf8_lossy(&o.stderr))))}}

pub struct GitStatus;
#[async_trait] impl Tool for GitStatus {fn name(&self)->&str{"git_status"}fn description(&self)->&str{"Inspect repository status without modifying it."}fn kind(&self)->ToolKind{ToolKind::ReadOnly}fn input_schema(&self)->Value{json!({"type":"object","properties":{},"required":[],"additionalProperties":false})}async fn execute(&self,c:&ToolContext,_:Value)->Result<String>{let o=Command::new("git").args(["status","--short","--branch"]).current_dir(&c.workspace).output().await?;Ok(truncate(format!("exit={}\n{}",o.status.code().unwrap_or(-1),String::from_utf8_lossy(&o.stdout))))}}

pub struct GitDiff;
#[async_trait] impl Tool for GitDiff {fn name(&self)->&str{"git_diff"}fn description(&self)->&str{"Inspect the current git diff without modifying the repository."}fn kind(&self)->ToolKind{ToolKind::ReadOnly}fn input_schema(&self)->Value{json!({"type":"object","properties":{"cached":{"type":"boolean"}},"required":[],"additionalProperties":false})}async fn execute(&self,c:&ToolContext,i:Value)->Result<String>{let cached=i["cached"].as_bool().unwrap_or(false);let mut q=Command::new("git");q.args(if cached{vec!["diff","--cached","--"]}else{vec!["diff","--"]}).current_dir(&c.workspace);let o=q.output().await?;Ok(truncate(String::from_utf8_lossy(&o.stdout).into_owned()))}}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_workspace() -> PathBuf {
        let root = std::env::temp_dir().join(format!("x11-tools-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        root
    }

    async fn cleanup(root: &Path) { let _ = tokio::fs::remove_dir_all(root).await; }

    #[test]
    fn registry_contains_core_tools() {
        let r = ToolRegistry::builtins();
        for n in ["read_file","write_file","edit_file","list_files","shell","search","git","git_status","git_diff"] { assert!(r.get(n).is_some()); }
    }

    #[tokio::test]
    async fn parent_path_is_rejected() {
        let c = ToolContext { workspace: std::env::current_dir().unwrap() };
        assert!(c.path("../Cargo.toml", false).await.is_err());
    }

    #[tokio::test]
    async fn write_rejects_symlink_escape() {
        let root = temp_workspace().await;
        let outside = root.with_extension("outside");
        tokio::fs::write(&outside, "secret").await.unwrap();
        #[cfg(unix)] {
            let link = root.join("link.txt");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            let c = ToolContext { workspace: root.clone() };
            assert!(c.path("link.txt", true).await.is_err());
        }
        cleanup(&root).await;
        let _ = tokio::fs::remove_file(&outside).await;
    }

    #[tokio::test]
    async fn write_limits_file_size() {
        let root = temp_workspace().await;
        let c = ToolContext { workspace: root.clone() };
        let result = WriteFile.execute(&c, json!({"path":"big.txt","content":"x".repeat(MAX_FILE_WRITE+1)})).await;
        assert!(result.is_err());
        cleanup(&root).await;
    }

    #[tokio::test]
    async fn edit_requires_exactly_one_match() {
        let root = temp_workspace().await;
        let c = ToolContext { workspace: root.clone() };
        WriteFile.execute(&c, json!({"path":"a.txt","content":"foo foo"})).await.unwrap();
        assert!(EditFile.execute(&c, json!({"path":"a.txt","old":"foo","new":"bar"})).await.is_err());
        let content = tokio::fs::read_to_string(root.join("a.txt")).await.unwrap();
        assert_eq!(content, "foo foo");
        cleanup(&root).await;
    }

    #[tokio::test]
    async fn write_replaces_existing_file() {
        let root = temp_workspace().await;
        let c = ToolContext { workspace: root.clone() };
        WriteFile.execute(&c, json!({"path":"a.txt","content":"one"})).await.unwrap();
        WriteFile.execute(&c, json!({"path":"a.txt","content":"two"})).await.unwrap();
        assert_eq!(tokio::fs::read_to_string(root.join("a.txt")).await.unwrap(), "two");
        cleanup(&root).await;
    }

    #[tokio::test]
    async fn shell_timeout_is_enforced() {
        let root = temp_workspace().await;
        let c = ToolContext { workspace: root.clone() };
        let command = if cfg!(windows) { "ping -n 3 127.0.0.1 > nul" } else { "sleep 2" };
        let result = Shell.execute(&c, json!({"command":command,"timeout_ms":100})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
        cleanup(&root).await;
    }

    #[tokio::test]
    async fn git_rejects_non_string_arguments() {
        let root = temp_workspace().await;
        let c = ToolContext { workspace: root.clone() };
        let result = Git.execute(&c, json!({"args":["status",42]})).await;
        assert!(result.is_err());
        cleanup(&root).await;
    }
}

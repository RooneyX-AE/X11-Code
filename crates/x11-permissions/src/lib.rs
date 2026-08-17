use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision { Allow, Deny, Ask }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation { Read, FilesystemWrite, Shell, GitWrite, Network }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule { pub decision: Decision, pub operation: Option<Operation>, pub pattern: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy { pub read: Decision, pub shell: Decision, pub filesystem_write: Decision, pub network: Decision, pub git_write: Decision, #[serde(default)] pub rules: Vec<Rule> }
impl Default for Policy { fn default()->Self{Self{read:Decision::Allow,shell:Decision::Ask,filesystem_write:Decision::Ask,network:Decision::Ask,git_write:Decision::Ask,rules:Vec::new()}}}
impl Policy {
 pub fn decide(&self, op:Operation)->Decision { self.rules.iter().rev().find_map(|r|{if r.operation.is_some_and(|o|o!=op){None}else{Some(r.decision)}}).unwrap_or_else(||match op{Operation::Read=>self.read,Operation::Shell=>self.shell,Operation::FilesystemWrite=>self.filesystem_write,Operation::Network=>self.network,Operation::GitWrite=>self.git_write}) }
 pub fn decide_for(&self, op:Operation, subject:&str)->Decision { self.rules.iter().rev().find_map(|r|{if r.operation.is_some_and(|o|o!=op){return None;}match &r.pattern{Some(pattern) if wildcard_match(pattern,subject)=>Some(r.decision),None=>Some(r.decision),_=>None}}).unwrap_or_else(||self.decide(op)) }
}
fn wildcard_match(pattern:&str,value:&str)->bool{if pattern=="*"{return true;}let parts:Vec<&str>=pattern.split('*').collect();if parts.len()==1{return pattern==value;}let mut cursor=0usize;for(index,part)in parts.iter().enumerate(){if part.is_empty(){continue;}let Some(found)=value[cursor..].find(part)else{return false;};if index==0&&found!=0{return false;}cursor+=found+part.len();}pattern.ends_with('*')||cursor==value.len()}

#[cfg(test)] mod tests{
 use super::*;
 #[test]fn safe_defaults(){let p=Policy::default();assert_eq!(p.decide(Operation::Read),Decision::Allow);assert_eq!(p.decide(Operation::Shell),Decision::Ask);assert_eq!(p.decide(Operation::FilesystemWrite),Decision::Ask);assert_eq!(p.decide(Operation::GitWrite),Decision::Ask);assert_eq!(p.decide(Operation::Network),Decision::Ask);}
 #[test]fn rules_override_defaults(){let mut p=Policy::default();p.rules.push(Rule{decision:Decision::Deny,operation:Some(Operation::Shell),pattern:Some("rm -rf*".into())});assert_eq!(p.decide_for(Operation::Shell,"rm -rf build"),Decision::Deny);assert_eq!(p.decide_for(Operation::Shell,"cargo test"),Decision::Ask);}
 #[test]fn latest_matching_rule_wins(){let mut p=Policy::default();p.rules.push(Rule{decision:Decision::Deny,operation:Some(Operation::Shell),pattern:Some("git *".into())});p.rules.push(Rule{decision:Decision::Allow,operation:Some(Operation::Shell),pattern:Some("git status".into())});assert_eq!(p.decide_for(Operation::Shell,"git status"),Decision::Allow);assert_eq!(p.decide_for(Operation::Shell,"git push"),Decision::Deny);}
 #[test]fn operation_scoping_is_strict(){let mut p=Policy::default();p.rules.push(Rule{decision:Decision::Deny,operation:Some(Operation::Shell),pattern:Some("write*".into())});assert_eq!(p.decide_for(Operation::FilesystemWrite,"write_file"),Decision::Ask);assert_eq!(p.decide_for(Operation::Shell,"write command"),Decision::Deny);}
 #[test]fn wildcard_edges_are_not_overbroad(){let mut p=Policy::default();p.rules.push(Rule{decision:Decision::Deny,operation:Some(Operation::Shell),pattern:Some("cargo test".into())});assert_eq!(p.decide_for(Operation::Shell,"cargo test"),Decision::Deny);assert_eq!(p.decide_for(Operation::Shell,"cargo tester"),Decision::Ask);assert_eq!(p.decide_for(Operation::Shell,"x cargo test"),Decision::Ask);}
 #[test]fn global_rule_applies_to_all_operations(){let mut p=Policy::default();p.rules.push(Rule{decision:Decision::Deny,operation:None,pattern:Some("secret*".into())});assert_eq!(p.decide_for(Operation::Read,"secret.txt"),Decision::Deny);assert_eq!(p.decide_for(Operation::Shell,"secret command"),Decision::Deny);}
}

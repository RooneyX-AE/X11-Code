use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision { Allow, Deny, Ask }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation { Read, FilesystemWrite, Shell, GitWrite, Network }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy { pub read: Decision, pub shell: Decision, pub filesystem_write: Decision, pub network: Decision, pub git_write: Decision }
impl Default for Policy { fn default()->Self{Self{read:Decision::Allow,shell:Decision::Ask,filesystem_write:Decision::Ask,network:Decision::Ask,git_write:Decision::Ask}} }
impl Policy { pub fn decide(&self, op:Operation)->Decision{match op{Operation::Read=>self.read,Operation::Shell=>self.shell,Operation::FilesystemWrite=>self.filesystem_write,Operation::Network=>self.network,Operation::GitWrite=>self.git_write}} }
#[cfg(test)] mod tests { use super::*; #[test] fn safe_defaults(){let p=Policy::default();assert_eq!(p.decide(Operation::Read),Decision::Allow);assert_eq!(p.decide(Operation::Shell),Decision::Ask);assert_eq!(p.decide(Operation::FilesystemWrite),Decision::Ask);} }

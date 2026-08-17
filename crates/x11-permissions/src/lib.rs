use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision { Allow, Deny, Ask }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub shell: Decision,
    pub filesystem_write: Decision,
    pub network: Decision,
    pub git_write: Decision,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            shell: Decision::Ask,
            filesystem_write: Decision::Ask,
            network: Decision::Ask,
            git_write: Decision::Ask,
        }
    }
}

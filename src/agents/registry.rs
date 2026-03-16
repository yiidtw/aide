use std::collections::HashMap;
use tracing::info;

use crate::config::AgentDef;

#[derive(Debug)]
pub struct AgentState {
    pub name: String,
    pub email: String,
    pub role: String,
    pub status: AgentStatus,
}

#[derive(Debug, PartialEq)]
pub enum AgentStatus {
    Stopped,
    Running,
    Error(String),
}

pub struct AgentRegistry {
    agents: HashMap<String, AgentState>,
}

impl AgentRegistry {
    pub fn from_config(defs: &HashMap<String, AgentDef>) -> Self {
        let agents = defs
            .iter()
            .map(|(name, def)| {
                let state = AgentState {
                    name: name.clone(),
                    email: def.email.clone(),
                    role: def.role.clone(),
                    status: AgentStatus::Stopped,
                };
                (name.clone(), state)
            })
            .collect();
        Self { agents }
    }

    pub fn start(&mut self, name: &str) -> bool {
        if let Some(agent) = self.agents.get_mut(name) {
            agent.status = AgentStatus::Running;
            info!(agent = %name, "agent started");
            true
        } else {
            false
        }
    }

    pub fn stop(&mut self, name: &str) -> bool {
        if let Some(agent) = self.agents.get_mut(name) {
            agent.status = AgentStatus::Stopped;
            info!(agent = %name, "agent stopped");
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> impl Iterator<Item = &AgentState> {
        self.agents.values()
    }
}

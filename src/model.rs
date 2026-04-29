use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Section {
    #[serde(default)]
    pub subtitle: String,
    #[serde(default, alias = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub content: serde_json::Value,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct TicketRaw {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: String,
    #[serde(
        default,
        alias = "estimate",
        alias = "estimate_time",
        alias = "estimateTime"
    )]
    pub estimate: String,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub struct Ticket {
    pub title: String,
    pub path: PathBuf,
    pub raw: TicketRaw,
    pub children: Vec<Ticket>,
}

impl Ticket {
    #[must_use]
    pub fn raw_is_empty(&self) -> bool {
        self.raw.description.is_empty() && self.raw.sections.is_empty()
    }

    #[must_use]
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

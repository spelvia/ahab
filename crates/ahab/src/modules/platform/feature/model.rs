//! The core features, and which may be switched off.

/// A core feature: its ID, name, what it is, and whether it may be off.
pub struct Feature {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Sessions is what the rest of Ahab is built around: always on.
    pub required: bool,
}

pub const FEATURES: [Feature; 5] = [
    Feature {
        id: "sessions",
        name: "Sessions",
        description: "Agents and terminals at work in a project, in folders.",
        required: true,
    },
    Feature {
        id: "terminal",
        name: "Terminal",
        description: "Terminals on your machines, kept running across restarts.",
        required: false,
    },
    Feature {
        id: "files",
        name: "Files",
        description: "The file tree and quick open. Files linked from sessions still open.",
        required: false,
    },
    Feature {
        id: "git",
        name: "Git",
        description: "Changes, commits, branches, history and the branch in the title bar.",
        required: false,
    },
    Feature {
        id: "search",
        name: "Search",
        description: "Searching a project's files.",
        required: false,
    },
];

pub fn find(id: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|feature| feature.id == id)
}

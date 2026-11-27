//! The sections of settings, their fields, and how values combine.

use serde_json::{Map, Value};

use crate::shared::error::{DomainError, FieldError};

/// Where values apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Everyone on this server (each person, until Ahab has users).
    Server,
    /// One project, over the server's.
    Project,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Project => "project",
        }
    }
}

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Boolean(bool),
    Integer {
        default: i64,
        min: i64,
        max: i64,
    },
    Choice {
        default: &'static str,
        options: &'static [&'static str],
    },
}

/// One setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Its name in values, camelCase; never renamed.
    pub name: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    /// It may be set per project too, not only for the server.
    pub per_project: bool,
}

impl Field {
    pub fn default_value(&self) -> Value {
        match self.kind {
            Kind::Boolean(default) => Value::Bool(default),
            Kind::Integer { default, .. } => Value::from(default),
            Kind::Choice { default, .. } => Value::from(default),
        }
    }

    /// Whether `value` fits this field.
    fn check(&self, value: &Value) -> Result<(), String> {
        match self.kind {
            Kind::Boolean(_) if value.is_boolean() => Ok(()),
            Kind::Boolean(_) => Err("Use true or false.".to_owned()),
            Kind::Integer { min, max, .. } => match value.as_i64() {
                Some(number) if (min..=max).contains(&number) => Ok(()),
                _ => Err(format!("Use a whole number from {min} to {max}.")),
            },
            Kind::Choice { options, .. } => match value.as_str() {
                Some(choice) if options.contains(&choice) => Ok(()),
                _ => Err(format!("Use one of {}.", options.join(", "))),
            },
        }
    }
}

/// A section of settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    /// Its ID in the API, such as `editor`; never renamed.
    pub id: &'static str,
    pub name: &'static str,
    pub fields: &'static [Field],
}

/// The editor's settings.
pub const EDITOR: Section = Section {
    id: "editor",
    name: "Editor",
    fields: &[
        Field {
            name: "editableByDefault",
            description: "Files open editable, instead of read-only.",
            kind: Kind::Boolean(false),
            per_project: true,
        },
        Field {
            name: "autoSave",
            description: "Save edited files by themselves: off, a moment after typing stops, or when the window loses focus.",
            kind: Kind::Choice {
                default: "off",
                options: &["off", "afterDelay", "onFocusChange"],
            },
            per_project: false,
        },
        Field {
            name: "autoSaveDelay",
            description: "How long after typing stops an edited file is saved, in milliseconds.",
            kind: Kind::Integer {
                default: 1000,
                min: 200,
                max: 60_000,
            },
            per_project: false,
        },
        Field {
            name: "vim",
            description: "Vim key bindings; :w saves.",
            kind: Kind::Boolean(false),
            per_project: false,
        },
        Field {
            name: "chatWhileWorking",
            description: "A message sent while the agent works: queued until its turn ends, or sent at once by stopping the turn. The composer can choose otherwise for each message.",
            kind: Kind::Choice {
                default: "queue",
                options: &["queue", "interrupt"],
            },
            per_project: false,
        },
        Field {
            name: "chatVim",
            description: "Vim key bindings in the chat composer too. Enter sends from either mode.",
            kind: Kind::Boolean(false),
            per_project: false,
        },
        Field {
            name: "fontSize",
            description: "The text's size, in pixels.",
            kind: Kind::Integer {
                default: 13,
                min: 9,
                max: 32,
            },
            per_project: false,
        },
        Field {
            name: "lineWrapping",
            description: "Wrap long lines instead of scrolling sideways.",
            kind: Kind::Boolean(false),
            per_project: false,
        },
        Field {
            name: "lineNumbers",
            description: "Show line numbers.",
            kind: Kind::Boolean(true),
            per_project: false,
        },
        Field {
            name: "indentationGuides",
            description: "Show indentation guides.",
            kind: Kind::Boolean(true),
            per_project: false,
        },
        Field {
            name: "problemsAtLineEnd",
            description: "Show each line's worst problem at its end, as well as underlined.",
            kind: Kind::Boolean(false),
            per_project: false,
        },
        Field {
            name: "tabSize",
            description: "How many columns a tab takes, and an indentation step.",
            kind: Kind::Integer {
                default: 4,
                min: 1,
                max: 16,
            },
            per_project: true,
        },
    ],
};

pub const SECTIONS: &[Section] = &[EDITOR];

pub fn find(id: &str) -> Option<&'static Section> {
    SECTIONS.iter().find(|section| section.id == id)
}

/// The values after a change at `scope`: `change` names fields with new
/// values, or `null` to unset them there. Unknown fields, values that do
/// not fit and fields not set per project are refused.
pub fn apply(
    section: &Section,
    scope: Scope,
    current: &Map<String, Value>,
    change: &Map<String, Value>,
) -> Result<Map<String, Value>, DomainError> {
    let mut errors = Vec::new();
    let mut values = current.clone();
    for (name, value) in change {
        let Some(field) = section.fields.iter().find(|field| field.name == name) else {
            errors.push(FieldError::new(
                name.clone(),
                format!("{} has no setting {name}.", section.name),
            ));
            continue;
        };
        if scope == Scope::Project && !field.per_project {
            errors.push(FieldError::new(
                field.name,
                "This is set for the server, not per project.",
            ));
            continue;
        }
        if value.is_null() {
            values.remove(name);
            continue;
        }
        match field.check(value) {
            Ok(()) => {
                values.insert(name.clone(), value.clone());
            }
            Err(message) => errors.push(FieldError::new(field.name, message)),
        }
    }
    if errors.is_empty() {
        Ok(values)
    } else {
        Err(DomainError::Invalid(errors))
    }
}

/// The values in effect: each field's default, then each layer's value
/// (widest first) when it fits; stored values that no longer fit are
/// skipped.
pub fn effective(section: &Section, layers: &[&Map<String, Value>]) -> Map<String, Value> {
    section
        .fields
        .iter()
        .map(|field| {
            let value = layers
                .iter()
                .rev()
                .find_map(|layer| {
                    layer
                        .get(field.name)
                        .filter(|value| field.check(value).is_ok())
                })
                .cloned()
                .unwrap_or_else(|| field.default_value());
            (field.name.to_owned(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn the_narrowest_scope_wins_over_the_defaults() {
        let server = map(json!({ "fontSize": 15, "editableByDefault": true }));
        let project = map(json!({ "editableByDefault": false, "tabSize": 2 }));
        let values = effective(&EDITOR, &[&server, &project]);
        assert_eq!(values["fontSize"], 15);
        assert_eq!(values["editableByDefault"], false);
        assert_eq!(values["tabSize"], 2);
        assert_eq!(values["vim"], false);
        assert_eq!(values["autoSave"], "off");
        // A stored value that no longer fits is skipped.
        let odd = map(json!({ "fontSize": "huge" }));
        assert_eq!(effective(&EDITOR, &[&odd])["fontSize"], 13);
    }

    #[test]
    fn changes_are_checked_and_null_unsets() {
        let current = map(json!({ "fontSize": 15 }));
        let changed = apply(
            &EDITOR,
            Scope::Server,
            &current,
            &map(json!({ "fontSize": null, "vim": true })),
        )
        .unwrap();
        assert_eq!(Value::Object(changed), json!({ "vim": true }));
        for bad in [
            json!({ "fontSize": 200 }),
            json!({ "vim": "yes" }),
            json!({ "autoSave": "always" }),
            json!({ "nope": 1 }),
        ] {
            assert!(
                apply(&EDITOR, Scope::Server, &current, &map(bad.clone())).is_err(),
                "{bad}"
            );
        }
        assert!(
            apply(
                &EDITOR,
                Scope::Project,
                &current,
                &map(json!({ "vim": true }))
            )
            .is_err()
        );
        assert!(
            apply(
                &EDITOR,
                Scope::Project,
                &current,
                &map(json!({ "editableByDefault": true }))
            )
            .is_ok()
        );
    }
}

//! Ahab's own settings on the wire.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::modules::platform::setting::model::{Field, Kind};
use crate::modules::platform::setting::service::Values;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SettingKind)]
pub enum SettingKindDto {
    Boolean,
    Integer,
    Choice,
    #[serde(other)]
    Other,
}

/// One setting of a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SettingField)]
pub struct SettingFieldDto {
    #[schema(example = "fontSize")]
    pub name: String,
    pub description: String,
    pub kind: SettingKindDto,
    #[schema(value_type = Object)]
    pub default: Value,
    /// Integers: the smallest allowed.
    pub minimum: Option<i64>,
    /// Integers: the largest allowed.
    pub maximum: Option<i64>,
    /// Choices: the allowed values.
    pub options: Vec<String>,
    /// It may be set per project too.
    pub per_project: bool,
}

impl From<&Field> for SettingFieldDto {
    fn from(field: &Field) -> Self {
        let (kind, minimum, maximum, options) = match field.kind {
            Kind::Boolean(_) => (SettingKindDto::Boolean, None, None, Vec::new()),
            Kind::Integer { min, max, .. } => {
                (SettingKindDto::Integer, Some(min), Some(max), Vec::new())
            }
            Kind::Choice { options, .. } => (
                SettingKindDto::Choice,
                None,
                None,
                options.iter().map(|&option| option.to_owned()).collect(),
            ),
        };
        Self {
            name: field.name.to_owned(),
            description: field.description.to_owned(),
            kind,
            default: field.default_value(),
            minimum,
            maximum,
            options,
            per_project: field.per_project,
        }
    }
}

/// A section of Ahab's settings: its fields, the values in effect, and the
/// values set for the server and for the project asked about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Settings)]
pub struct SettingsDto {
    #[schema(example = "editor")]
    pub section: String,
    pub fields: Vec<SettingFieldDto>,
    /// Every field's value in effect: the project's, else the server's,
    /// else its default.
    #[schema(value_type = Object)]
    pub values: Map<String, Value>,
    /// The values set for the server.
    #[schema(value_type = Object)]
    pub server: Map<String, Value>,
    /// The values set for the project, when one was asked about.
    #[schema(value_type = Option<Object>)]
    pub project: Option<Map<String, Value>>,
}

impl From<Values> for SettingsDto {
    fn from(values: Values) -> Self {
        Self {
            section: values.section.id.to_owned(),
            fields: values
                .section
                .fields
                .iter()
                .map(SettingFieldDto::from)
                .collect(),
            values: values.effective,
            server: values.server,
            project: values.project,
        }
    }
}

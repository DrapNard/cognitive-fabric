//! Host advisory selectors for values produced by typed expert operations.
//!
//! These selectors identify an operation result and an optional JSON pointer.
//! They do not carry provider, model, capability, or authority information.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const MAX_OPERATION_ID_BYTES: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Input {
    Whole(String),
    Selection(SelectionDto),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SelectionDto {
    operation_id: String,
    #[serde(default)]
    pointer: String,
}

impl Input {
    /// Returns the source operation identifier.
    pub(super) fn operation_id(&self) -> &str {
        match self {
            Self::Whole(operation_id) => operation_id,
            Self::Selection(selection) => &selection.operation_id,
        }
    }

    /// Returns the JSON pointer, or an empty pointer for the whole result.
    pub(super) fn pointer(&self) -> &str {
        match self {
            Self::Whole(_) => "",
            Self::Selection(selection) => &selection.pointer,
        }
    }

    /// Validates the operation identifier and bounded RFC 6901 pointer syntax.
    pub(super) fn validate(&self) -> Result<()> {
        validate_operation_id(self.operation_id())?;
        super::typed_experts::validate_source_pointer(self.pointer())
    }
}

fn validate_operation_id(operation_id: &str) -> Result<()> {
    let bytes = operation_id.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_OPERATION_ID_BYTES
        || !bytes[0].is_ascii_alphabetic()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        bail!("Typed input operation ID must be 1–32 ASCII letters, digits, or underscores and start with a letter")
    }
    Ok(())
}

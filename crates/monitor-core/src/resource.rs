// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Add-resource validation (DESIGN §7, Q7): a resource is monitorable only if it is a stealth
//! resource, is viewable, and its view key matches one we hold.

use tari_ootle_common_types::engine_types::resource::Resource;
use tari_template_lib_types::ResourceType;

use crate::types::ViewKeyRow;

#[derive(Debug, thiserror::Error)]
pub enum ResourceValidationError {
    #[error("not a stealth resource (resource type is {resource_type:?}); only stealth resources have UTXOs")]
    NotStealth { resource_type: ResourceType },
    #[error("resource is not viewable (it embeds no view key)")]
    NotViewable,
    #[error("you don't hold the view key for this resource")]
    NoMatchingViewKey,
}

/// Validates a resource for monitoring and returns the stored view key whose public matches the
/// resource's embedded view key. Each of the three gates fails with a distinct error.
pub fn find_matching_view_key<'a>(
    resource: &Resource,
    view_keys: &'a [ViewKeyRow],
) -> Result<&'a ViewKeyRow, ResourceValidationError> {
    let resource_type = resource.resource_type();
    if !resource_type.is_stealth() {
        return Err(ResourceValidationError::NotStealth { resource_type });
    }
    let view_key = resource.view_key().ok_or(ResourceValidationError::NotViewable)?;
    view_keys
        .iter()
        .find(|k| k.public == *view_key)
        .ok_or(ResourceValidationError::NoMatchingViewKey)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tari_template_lib_types::{
        Metadata,
        SubstateOwnerRule,
        access_rules::ResourceAccessRules,
        crypto::RistrettoPublicKeyBytes,
    };

    use super::*;

    fn resource(resource_type: ResourceType, view_key: Option<RistrettoPublicKeyBytes>) -> Resource {
        Resource::new(
            resource_type,
            SubstateOwnerRule::None,
            ResourceAccessRules::default(),
            Metadata::default(),
            view_key,
            None,
            6,
            false,
        )
    }

    fn key_row(n: u8) -> ViewKeyRow {
        ViewKeyRow {
            id: n as i64,
            label: format!("key-{n}"),
            secret: [n; 32],
            public: RistrettoPublicKeyBytes::from_bytes(&[n; 32]).unwrap(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn rejects_non_stealth_resources() {
        let resource = resource(ResourceType::Fungible, Some(key_row(1).public));
        let err = find_matching_view_key(&resource, &[key_row(1)]).unwrap_err();
        assert!(matches!(err, ResourceValidationError::NotStealth { .. }));
    }

    #[test]
    fn rejects_resources_without_a_view_key() {
        let resource = resource(ResourceType::Stealth, None);
        let err = find_matching_view_key(&resource, &[key_row(1)]).unwrap_err();
        assert!(matches!(err, ResourceValidationError::NotViewable));
    }

    #[test]
    fn rejects_when_no_stored_key_matches() {
        let resource = resource(ResourceType::Stealth, Some(key_row(9).public));
        let err = find_matching_view_key(&resource, &[key_row(1), key_row(2)]).unwrap_err();
        assert!(matches!(err, ResourceValidationError::NoMatchingViewKey));
    }

    #[test]
    fn returns_the_matching_key() {
        let resource = resource(ResourceType::Stealth, Some(key_row(2).public));
        let keys = [key_row(1), key_row(2)];
        let key = find_matching_view_key(&resource, &keys).unwrap();
        assert_eq!(key.id, 2);
    }
}

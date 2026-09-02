//! See [`super`]. One family of workflow-check rules.

use super::*;

pub(crate) fn permission_label(permissions: yunta_core::NodePermissions) -> &'static str {
    match permissions {
        yunta_core::NodePermissions::ReadOnly => "read-only",
        yunta_core::NodePermissions::Edit => "edit",
        yunta_core::NodePermissions::Full => "full",
    }
}

pub(crate) fn permission_rank(permissions: yunta_core::NodePermissions) -> u8 {
    match permissions {
        yunta_core::NodePermissions::ReadOnly => 0,
        yunta_core::NodePermissions::Edit => 1,
        yunta_core::NodePermissions::Full => 2,
    }
}

/// `declares.permissions` is a ceiling, checked against
/// every `prompt`/`loop` node's own *effective* permission — the node's
/// explicit `permissions:` if it has one, the engine's own `edit`
/// default otherwise (a pack declaring `read-only` can't rely on a node
/// silently defaulting past it). A no-op for a repo-origin workflow:
/// there is no pack manifest to hold a ceiling against.
pub(crate) fn check_declares_ceiling(
    workflow: &Workflow,
    origin: &crate::catalog::WorkflowOrigin,
    repo_root: &std::path::Path,
) -> Vec<CheckError> {
    let crate::catalog::WorkflowOrigin::Pack {
        publisher,
        pack_name,
    } = origin
    else {
        return Vec::new();
    };
    let manifest_path = repo_root
        .join(".yunta/packs")
        .join(publisher.as_str())
        .join(pack_name.as_str())
        .join("pack.yaml");
    // The manifest is this pack's only governance rule; a copy that
    // cannot be read or parsed is a check error naming the file, never a
    // ceiling silently switched off.
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(e) => {
            return vec![CheckError::PackManifestUnreadable {
                path: manifest_path,
                detail: e.to_string(),
            }]
        }
    };
    let manifest = match yunta_core::yaml::parse::<yunta_core::PackManifest>(&text) {
        Ok(manifest) => manifest,
        Err(e) => {
            return vec![CheckError::PackManifestMalformed {
                path: manifest_path,
                detail: e.to_string(),
            }]
        }
    };
    let ceiling = manifest.declares.permissions;

    workflow
        .iter_nodes()
        .filter_map(|node| {
            let effective = match &node.kind {
                NodeKind::Prompt { .. } | NodeKind::Loop { .. } => node
                    .permissions
                    .unwrap_or(yunta_core::NodePermissions::Edit),
                _ => return None,
            };
            if permission_rank(effective) <= permission_rank(ceiling) {
                return None;
            }
            Some(CheckError::PackPermissionsCeilingExceeded {
                node: node.id.clone(),
                pack: format!("{publisher}/{pack_name}"),
                declared: permission_label(ceiling),
                effective: permission_label(effective),
            })
        })
        .collect()
}

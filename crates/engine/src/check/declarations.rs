//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Config values the schema parses but nothing implements yet
/// must be refused, never accepted and ignored. Today that is
/// `defaults.on_failure` beyond `pause` (the built behavior), and a
/// `defaults.runner` that `runners:` doesn't define.
pub(crate) fn check_config_defaults(config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    let Some(defaults) = &config.defaults else {
        return;
    };
    if defaults.max_parallel_nodes == Some(0) {
        errors.push(CheckError::MaxParallelNodesZero);
    }
    if let Some(on_failure) = defaults.on_failure {
        if on_failure != yunta_core::DefaultOnFailure::Pause {
            errors.push(CheckError::DefaultOnFailureUnsupported { on_failure });
        }
    }
    if let Some(runner) = &defaults.runner {
        let defined = config
            .runners
            .as_ref()
            .and_then(|runners| runners.get(runner))
            .is_some_and(|candidates| !candidates.is_empty());
        if !defined {
            errors.push(CheckError::UnknownRunner {
                node: DEFAULTS.clone(),
                runner: runner.clone(),
            });
        }
    }
}

/// `on_interrupt: resume_session` declared on a node that opens no
/// session is refused; the *config default* stays legal (it applies
/// where a session exists and means restart everywhere else).
pub(crate) fn check_resume_session(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for node in workflow.iter_nodes() {
        if node.on_interrupt == Some(yunta_core::OnInterrupt::ResumeSession)
            && !matches!(node.kind, NodeKind::Prompt { .. })
        {
            errors.push(CheckError::ResumeSessionOnSessionlessNode {
                node: node.id.clone(),
            });
        }
    }
}

/// `yunta_schema` is a space-separated list of comparators
/// over the schema major (`>=1 <2`, `=1`, `<3`…), all of which must
/// hold for [`yunta_core::YUNTA_SCHEMA`]. Deliberately a ~20-line
/// parser instead of a semver dependency: the schema version is one
/// integer, and the small static binary is a product feature.
pub(crate) fn check_yunta_schema(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let Some(range) = &workflow.yunta_schema else {
        return;
    };
    match yunta_schema_satisfied(range, yunta_core::YUNTA_SCHEMA) {
        Ok(true) => {}
        Ok(false) => errors.push(CheckError::YuntaSchemaOutside {
            range: range.clone(),
            binary: yunta_core::YUNTA_SCHEMA,
        }),
        Err(source) => errors.push(CheckError::YuntaSchemaUnreadable {
            range: range.clone(),
            binary: yunta_core::YUNTA_SCHEMA,
            source,
        }),
    }
}

/// `Ok(bool)` = every comparator evaluated against `binary`; `Err` = the
/// range doesn't parse. Empty ranges don't parse either — a declared
/// requirement that constrains nothing is a typo, not a wildcard.
pub(crate) fn yunta_schema_satisfied(range: &str, binary: u32) -> Result<bool, SchemaRangeError> {
    let mut any = false;
    for comparator in range.split_whitespace() {
        let (op, number) = comparator
            .find(|c: char| c.is_ascii_digit())
            .map(|i| comparator.split_at(i))
            .ok_or_else(|| SchemaRangeError::NoVersion {
                comparator: comparator.to_string(),
            })?;
        let number: u32 = number.parse().map_err(|_| SchemaRangeError::NotAVersion {
            text: number.to_string(),
        })?;
        let holds = match op {
            ">=" => binary >= number,
            "<=" => binary <= number,
            ">" => binary > number,
            "<" => binary < number,
            "=" | "==" | "" => binary == number,
            other => {
                return Err(SchemaRangeError::UnknownOperator {
                    op: other.to_string(),
                })
            }
        };
        any = true;
        if !holds {
            return Ok(false);
        }
    }
    if !any {
        return Err(SchemaRangeError::Empty);
    }
    Ok(true)
}

/// Every declared artifact name stays under the run's `artifacts/`:
/// relative, with no `..` component. Templates in a name
/// (`findings-{{runner.role}}.yaml`) are checked as written.
pub(crate) fn check_artifact_names(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for node in workflow.iter_nodes() {
        let Some(artifacts) = &node.artifacts else {
            continue;
        };
        for spec in &artifacts.produces {
            let name = match spec {
                yunta_core::ArtifactSpec::Plain(name) => name,
                yunta_core::ArtifactSpec::Typed { name, .. } => name,
            };
            if !yunta_core::stays_inside(name) {
                errors.push(CheckError::ArtifactNameEscapes {
                    node: node.id.clone(),
                    name: name.clone(),
                });
            }
        }
    }
}

/// Every `on_finish.distill` path must be some node's declared
/// artifact. Template-bearing names (`findings-{{runner.role}}.yaml`)
/// compare as written — the distill declaration must match the
/// production declaration, both pre-render.
pub(crate) fn check_distill_paths(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let mut produced: HashSet<&str> = HashSet::new();
    for node in workflow.iter_nodes() {
        if let Some(artifacts) = &node.artifacts {
            for spec in &artifacts.produces {
                produced.insert(match spec {
                    yunta_core::ArtifactSpec::Plain(name) => name,
                    yunta_core::ArtifactSpec::Typed { name, .. } => name,
                });
            }
        }
    }
    for step in &workflow.on_finish {
        let yunta_core::OnFinishStep::Distill { distill } = step else {
            continue;
        };
        for path in distill {
            if !produced.contains(path.as_str()) {
                errors.push(CheckError::DistillUnknownArtifact { path: path.clone() });
            }
        }
    }
}

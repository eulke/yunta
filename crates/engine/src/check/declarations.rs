//! See [`super`]. One family of workflow-check rules.

use super::*;
use yunta_core::events::ArtifactId;

/// Config `defaults:` values that only `check` can catch before a run:
/// a `max_parallel_nodes` of zero (which would schedule nothing), and a
/// `defaults.runner` that `runners:` doesn't define. Every
/// `defaults.on_failure` value is now built, so none is refused here.
pub(crate) fn check_config_defaults(config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    let Some(defaults) = &config.defaults else {
        return;
    };
    if defaults.max_parallel_nodes == Some(0) {
        errors.push(CheckError::MaxParallelNodesZero);
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
            && !node.kind.opens_resumable_session()
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

/// What a node may declare it produces: one document of each kind at
/// most, and opaque names that stay under the run's `artifacts/`.
///
/// The identity of an interpreted artifact is `(node, kind)`, so a node
/// declaring one kind twice is declaring one artifact twice and the
/// second declaration could never be answered separately. An opaque
/// artifact is written to a path, and a name that is absolute or climbs
/// with `..` would land outside the run — templates in a name
/// (`report-{{runner.role}}.md`) are checked as written.
pub(crate) fn check_artifact_declarations(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for node in workflow.iter_nodes() {
        let Some(artifacts) = &node.artifacts else {
            continue;
        };
        let mut kinds: HashSet<yunta_core::ArtifactKind> = HashSet::new();
        for spec in &artifacts.produces {
            match spec {
                yunta_core::ArtifactSpec::Interpreted(kind) => {
                    if !kinds.insert(*kind) {
                        errors.push(CheckError::DuplicateArtifactKind {
                            node: node.id.clone(),
                            kind: *kind,
                        });
                    }
                }
                yunta_core::ArtifactSpec::Opaque(name) => {
                    // The name as written, which is what a reader of the
                    // workflow sees: a template renders to a name like any
                    // other and is parsed again once it is known.
                    if let Err(problem) = yunta_core::ArtifactName::parse(name) {
                        errors.push(CheckError::ArtifactNameRefused {
                            node: node.id.clone(),
                            said: problem.to_string(),
                        });
                    }
                }
            }
        }
    }
}

/// A `document` input and a node producing that kind are two producers
/// of one identity, and nothing orders them.
///
/// The input is the answer for a workflow whose document comes from
/// outside — a person points the run at one, and no node stands in to
/// hand it over. A node that produces the same kind is a second answer
/// to the same question, and a reader asking the run for that kind
/// would get whichever acceptance landed last. Refused here, where both
/// declarations are in sight, rather than left to a run that silently
/// reads one of them.
pub(crate) fn check_input_documents(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for (input, kind) in workflow
        .inputs
        .iter()
        .filter_map(|(name, spec)| match spec {
            InputSpec::Document { kind, .. } => Some((name, *kind)),
            _ => None,
        })
    {
        for node in workflow.iter_nodes() {
            let produces = node
                .artifacts
                .iter()
                .flat_map(|artifacts| &artifacts.produces)
                .any(|spec| *spec == yunta_core::ArtifactSpec::Interpreted(kind));
            if produces {
                errors.push(CheckError::InputDocumentAlsoProduced {
                    input: input.clone(),
                    node: node.id.clone(),
                    kind,
                });
            }
        }
    }
}

/// Every `on_finish.distill` entry must be an artifact the node it names
/// declares.
pub(crate) fn check_distill_paths(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for step in &workflow.on_finish {
        let yunta_core::OnFinishStep::Distill { distill } = step else {
            continue;
        };
        for declaration in distill {
            let declares = workflow
                .iter_nodes()
                .filter(|node| node.id == declaration.node)
                .filter_map(|node| node.artifacts.as_ref())
                .flat_map(|artifacts| artifacts.produces.iter())
                .any(|spec| ArtifactId::from(spec) == ArtifactId::from(&declaration.id));
            if !declares {
                errors.push(CheckError::DistillUnknownArtifact {
                    artifact: declaration.clone(),
                });
            }
        }
    }
}

/// The three kind names are reserved as file names.
///
/// `tasks`, `findings` and `questions` are how a document the engine
/// reads is named — in `artifacts.produces`, a bare `tasks` *is* the
/// tasks document — so a reference spelling one of them as a `name:`
/// asks for an opaque artifact that nothing can ever produce. Caught
/// here rather than left to resolve to nothing at run time, and named
/// with the form that does work.
pub(crate) fn check_reserved_artifact_names(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let mut reserved = |site: String, name: &str| {
        if name.parse::<yunta_core::ArtifactKind>().is_ok() {
            errors.push(CheckError::ReservedArtifactName {
                site,
                name: name.to_string(),
            });
        }
    };
    for node in workflow.iter_nodes() {
        for source in &node.context {
            if let yunta_core::ContextSpec::Artifact { artifact } = source {
                if let yunta_core::ArtifactRefId::Name { name } = &artifact.id {
                    reserved(
                        format!("the `artifact:` context source of node `{}`", node.id),
                        name,
                    );
                }
            }
        }
        if let yunta_core::NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                let site = format!("a `mounts:` entry of node `{}`", node.id);
                if let yunta_core::ArtifactRefId::Name { name } = &mount.artifact.id {
                    reserved(site.clone(), name);
                }
                if let Some(name) = &mount.artifact.rename {
                    reserved(site, name);
                }
            }
        }
    }
    for step in &workflow.on_finish {
        if let yunta_core::OnFinishStep::Distill { distill } = step {
            for declaration in distill {
                if let yunta_core::ArtifactRefId::Name { name } = &declaration.id {
                    reserved("an `on_finish.distill` entry".to_string(), name);
                }
            }
        }
    }
}

use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A source that binds every name to the same value.
struct Fixed(&'static str);
impl SecretSource for Fixed {
    fn get(&self, _name: &str) -> Option<Secret<String>> {
        Some(Secret::new(self.0.to_string()))
    }
}

#[test]
fn a_secret_spelled_as_a_number_is_taken_out_of_the_number_field() {
    let redactor = Redactor::of(&["PORT".to_string()], &Fixed("1234"))
        .expect("four characters is the shortest a secret may be");
    assert_eq!(
        redactor.json(serde_json::json!({ "pid": 1234 })),
        serde_json::json!({ "pid": REDACTED })
    );
}

#[test]
fn a_secret_under_four_characters_is_refused_by_name() {
    let refused = Redactor::of(&["PORT".to_string()], &Fixed("abc"))
        .expect_err("three characters is shorter than the floor");
    assert_eq!(
        refused,
        ShortSecret {
            name: "PORT".to_string()
        }
    );
}

#[test]
fn the_log_filter_is_read_with_the_other_variables() {
    let read = Env::of_vars(
        &|name| (name == "RUST_LOG").then(|| "yunta=debug".into()),
        &[],
    );
    assert_eq!(read.rust_log.as_deref(), Some("yunta=debug"));
}

/// What an environment hands on to a child reads back, at the child's edge, as
/// that environment: the child stands where its parent's workspace stands.
#[test]
fn an_environment_handed_on_reads_back_as_itself() {
    let env = Env {
        home: Some(PathBuf::from("/home/u")),
        yunta_home: Some(PathBuf::from("/scratch/state")),
        org_config: None,
        path: SearchPath::new(vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")]),
        user: Some("ana".to_string()),
        terminal: BTreeMap::from([("TERM".to_string(), "xterm".to_string())]),
        service: BTreeMap::from([("HTTPS_PROXY".to_string(), "http://proxy".to_string())]),
        ..Env::default()
    };
    let handed = env.handed_on();
    let var = |name: &str| {
        handed
            .iter()
            .find(|(handed, _)| handed == name)
            .and_then(|(_, value)| value.clone())
    };
    assert_eq!(Env::of_vars(&var, &["HTTPS_PROXY"]), env);
    assert!(
        handed
            .iter()
            .any(|(name, value)| name == "YUNTA_ORG_CONFIG" && value.is_none()),
        "what it holds no value for, the child goes without"
    );
}

#[test]
fn yunta_home_overrides_home() {
    let env = Env {
        home: Some(PathBuf::from("/home/u")),
        yunta_home: Some(PathBuf::from("/scratch/state")),
        ..Default::default()
    };
    assert_eq!(user_state_root(&env), Some(PathBuf::from("/scratch/state")));
}

#[test]
fn home_falls_back_to_dot_yunta() {
    let env = Env {
        home: Some(PathBuf::from("/home/u")),
        ..Default::default()
    };
    assert_eq!(user_state_root(&env), Some(PathBuf::from("/home/u/.yunta")));
}

#[test]
fn neither_known_is_none() {
    assert_eq!(user_state_root(&Env::default()), None);
}

#[test]
fn knowledge_lives_under_the_yunta_home_state_root() {
    let env = Env {
        home: Some(PathBuf::from("/home/u")),
        yunta_home: Some(PathBuf::from("/scratch/state")),
        ..Default::default()
    };
    assert_eq!(
        user_knowledge_dir(&env),
        Some(PathBuf::from("/scratch/state/knowledge"))
    );
}

#[test]
fn knowledge_falls_back_to_dot_yunta_under_home() {
    let env = Env {
        home: Some(PathBuf::from("/home/u")),
        ..Default::default()
    };
    assert_eq!(
        user_knowledge_dir(&env),
        Some(PathBuf::from("/home/u/.yunta/knowledge"))
    );
}

#[test]
fn knowledge_is_none_when_neither_is_known() {
    assert_eq!(user_knowledge_dir(&Env::default()), None);
}

use rcompose_spec::interpolation::*;
use std::collections::HashMap;

#[test]
fn test_interpolation_basic() {
    let mut env = HashMap::new();
    env.insert("TAG".to_string(), "v1.0".to_string());
    env.insert("EMPTY".to_string(), "".to_string());

    assert_eq!(
        interpolate_string("image: app:${TAG}", &env).unwrap(),
        "image: app:v1.0"
    );
    assert_eq!(
        interpolate_string("port: ${PORT:-8080}", &env).unwrap(),
        "port: 8080"
    );
    assert_eq!(
        interpolate_string("val: ${EMPTY:-fallback}", &env).unwrap(),
        "val: fallback"
    );
    assert_eq!(
        interpolate_string("val: ${EMPTY-fallback}", &env).unwrap(),
        "val: "
    );
    assert_eq!(
        interpolate_string("price: $$100", &env).unwrap(),
        "price: $100"
    );
}

#[test]
fn test_interpolation_error() {
    let env = HashMap::new();
    let err = interpolate_string("db: ${DB_PASS:?password is required}", &env);
    assert!(err.is_err());
    let err_msg = err.unwrap_err().to_string();
    assert!(err_msg.contains("password is required"));
}

#[test]
fn test_env_file_parser() {
    let content = r#"
# Comments should be ignored
FOO=bar
BAZ="quoted value"
NUM=123
EMPTY=
"#;
    let env = parse_env_content(content);
    assert_eq!(env.get("FOO").map(|s| s.as_str()), Some("bar"));
    assert_eq!(env.get("BAZ").map(|s| s.as_str()), Some("quoted value"));
    assert_eq!(env.get("NUM").map(|s| s.as_str()), Some("123"));
    assert_eq!(env.get("EMPTY").map(|s| s.as_str()), Some(""));
}

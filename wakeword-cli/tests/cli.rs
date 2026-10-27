use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("wakeword-cli-{}-{}", std::process::id(), unique));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> (Output, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_wakeword-cli"))
            .args(args)
            .env("WAKEWORD_CONFIG", self.0.join("config.json"))
            .output()
            .unwrap();
        let value: Value = serde_json::from_slice(&output.stdout)
            .expect("stdout must contain exactly one JSON response");
        assert_eq!(value["api_version"], 1);
        (output, value)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn builtin_list_and_request_correlation() {
    let sandbox = Sandbox::new();
    let (output, value) = sandbox.run(&["--json", "--request-id", "ui-42", "list"]);
    assert!(output.status.success());
    assert_eq!(value["request_id"], "ui-42");
    assert_eq!(value["data"]["names"].as_array().unwrap().len(), 3);
    assert_eq!(value["data"]["selected"], "alisa");
    assert_eq!(value["data"]["capabilities"]["custom_training"], false);
}

#[test]
fn set_persists_choice_without_applying_or_corrupting_failed_choice() {
    let sandbox = Sandbox::new();
    let (output, value) = sandbox.run(&["set", "yasmina", "--json"]);
    assert!(output.status.success());
    assert_eq!(value["data"]["applied"], false);
    let before = fs::read(sandbox.0.join("config.json")).unwrap();
    let (output, value) = sandbox.run(&["--json", "switch", "unknown"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(value["error"]["code"], "unknown_name");
    assert_eq!(fs::read(sandbox.0.join("config.json")).unwrap(), before);
    let (_, value) = sandbox.run(&["current", "--json"]);
    assert_eq!(value["data"]["selected"], "yasmina");
}

#[test]
fn invalid_arguments_return_json_and_usage_exit_code() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["--json", "set"],
        vec!["--json", "list", "extra"],
        vec!["--json", "bogus"],
        vec!["--json", "--request-id"],
    ] {
        let (output, value) = sandbox.run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(value["ok"], false);
    }
}

#[test]
fn concurrent_writer_is_rejected_and_lock_recovers_after_drop() {
    let sandbox = Sandbox::new();
    let manager =
        wakeword_core::Manager::with_path(sandbox.0.join("config.json").to_str().unwrap());
    let lock = manager.try_lock().unwrap();
    let (output, value) = sandbox.run(&["--json", "set", "yandex"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(value["error"]["code"], "busy");
    assert!(!sandbox.0.join("config.json").exists());
    drop(lock);
    assert!(sandbox.run(&["--json", "set", "yandex"]).0.status.success());
}

#[test]
fn corrupt_config_is_not_reported_as_success() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.0.join("config.json"), "not JSON").unwrap();
    let (output, value) = sandbox.run(&["--json", "current"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(value["error"]["code"], "config_read_failed");
}

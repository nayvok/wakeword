use serde_json::{json, Value};
use std::env;
use std::process::ExitCode;
use wakeword_core::Manager;

struct Failure {
    code: &'static str,
    message: String,
    exit: u8,
}

type Result<T> = std::result::Result<T, Failure>;

fn failure(code: &'static str, error: impl std::fmt::Display) -> Failure {
    Failure {
        code,
        message: error.to_string(),
        exit: 1,
    }
}

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let json_output = take_flag(&mut args, "--json");
    let request_id = match take_value(&mut args, "--request-id") {
        Ok(value) => value,
        Err(error) => return emit("", json_output, None, Err(error)),
    };
    let command = args.first().map(String::as_str).unwrap_or("help");
    let mgr = env::var("WAKEWORD_CONFIG")
        .map(Manager::with_path)
        .unwrap_or_default();
    let result = dispatch(&mgr, &args);
    emit(command, json_output, request_id.as_deref(), result)
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Result<Option<String>> {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        if index >= args.len() || args[index].starts_with("--") {
            return Err(Failure {
                code: "invalid_arguments",
                message: format!("{} требует значение", flag),
                exit: 2,
            });
        }
        return Ok(Some(args.remove(index)));
    }
    Ok(None)
}

fn dispatch(mgr: &Manager, args: &[String]) -> Result<Value> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    let expected = match command {
        "help" | "--help" | "-h" | "version" | "--version" | "list" | "current" | "status"
        | "apply" => 1,
        "set" | "switch" | "remove" => 2,
        "add" => 3,
        _ => {
            return Err(Failure {
                code: "unknown_command",
                message: format!("неизвестная команда: {}", command),
                exit: 2,
            })
        }
    };
    if !args.is_empty() && args.len() != expected {
        return Err(Failure {
            code: "invalid_arguments",
            message: "неверное число аргументов; используйте wakeword help".into(),
            exit: 2,
        });
    }
    if matches!(command, "help" | "--help" | "-h") {
        return Ok(json!({"usage": USAGE}));
    }
    if matches!(command, "version" | "--version") {
        return Ok(json!({"version": env!("CARGO_PKG_VERSION")}));
    }
    let _lock = if matches!(command, "set" | "switch" | "apply" | "add" | "remove") {
        Some(mgr.try_lock().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                Failure {
                    code: "busy",
                    message: "другая команда уже изменяет конфигурацию".into(),
                    exit: 3,
                }
            } else {
                failure("lock_failed", error)
            }
        })?)
    } else {
        None
    };
    let mut cfg = mgr.load().map_err(|e| failure("config_read_failed", e))?;
    match command {
        "list" => Ok(json!({
            "selected": cfg.active,
            "names": Manager::list_names(&cfg).into_iter().map(|name| json!({"builtin": (["alisa", "yandex", "yasmina"].contains(&name.as_str())), "selected": name == cfg.active, "name": name})).collect::<Vec<_>>(),
            "capabilities": {"builtin_switching": true, "custom_training": false}
        })),
        "current" => Ok(json!({"selected": cfg.active})),
        "status" => serde_json::to_value(mgr.inspect().map_err(|e| failure("status_failed", e))?)
            .map_err(|e| failure("serialization_failed", e)),
        "set" | "switch" => {
            Manager::set_active(&mut cfg, &args[1]).map_err(|e| failure("unknown_name", e))?;
            if command == "switch" {
                mgr.switch(&args[1])
                    .map_err(|e| failure("apply_failed", e))?;
            } else {
                mgr.save(&cfg)
                    .map_err(|e| failure("config_write_failed", e))?;
            }
            Ok(json!({"selected": cfg.active, "applied": command == "switch"}))
        }
        "apply" => {
            mgr.apply().map_err(|e| failure("apply_failed", e))?;
            Ok(json!({"selected": cfg.active, "applied": true}))
        }
        // Retained for existing users; custom model workflows are outside the public v1 scope.
        "add" => {
            Manager::add_custom(&mut cfg, &args[1], &args[2])
                .map_err(|e| failure("invalid_model", e))?;
            mgr.save(&cfg)
                .map_err(|e| failure("config_write_failed", e))?;
            Ok(json!({"selected": cfg.active, "registered": args[1]}))
        }
        "remove" => {
            Manager::remove(&mut cfg, &args[1]).map_err(|e| failure("invalid_model", e))?;
            mgr.save(&cfg)
                .map_err(|e| failure("config_write_failed", e))?;
            Ok(json!({"selected": cfg.active, "removed": args[1]}))
        }
        _ => unreachable!(),
    }
}

fn emit(
    command: &str,
    json_output: bool,
    request_id: Option<&str>,
    result: Result<Value>,
) -> ExitCode {
    let (response, exit) = match result {
        Ok(data) => (
            json!({"api_version": 1, "ok": true, "command": command, "request_id": request_id, "data": data}),
            0,
        ),
        Err(error) => (
            json!({"api_version": 1, "ok": false, "command": command, "request_id": request_id, "error": {"code": error.code, "message": error.message}}),
            error.exit,
        ),
    };
    if json_output {
        println!("{}", response);
    } else if response["ok"] == false {
        eprintln!(
            "ошибка: {}",
            response["error"]["message"].as_str().unwrap_or("unknown")
        );
    } else {
        print_human(command, &response["data"]);
    }
    ExitCode::from(exit)
}

fn print_human(command: &str, data: &Value) {
    match command {
        "help" | "--help" | "-h" => println!("{}", USAGE),
        "version" | "--version" => println!("wakeword {}", env!("CARGO_PKG_VERSION")),
        "list" => {
            for name in data["names"].as_array().unwrap() {
                println!(
                    "{}{}",
                    name["name"].as_str().unwrap(),
                    if name["selected"] == true { " *" } else { "" }
                );
            }
        }
        "current" => println!("{}", data["selected"].as_str().unwrap()),
        "status" => {
            println!("выбрано: {}", data["selected"].as_str().unwrap());
            println!(
                "серверная проверка активации отключена: {}",
                data["server_activation_validation_disabled"]
            );
            for model in data["models"].as_array().unwrap() {
                println!(
                    "{}: {}",
                    model["path"].as_str().unwrap(),
                    model["version"].as_str().unwrap()
                );
                if let Some(matches) = model["checksum_matches"].as_bool() {
                    println!(
                        "  CRC32: {}",
                        if matches {
                            "OK"
                        } else {
                            "не совпадает"
                        }
                    );
                }
            }
            println!(
                "Проверены файлы. Загрузку движком и голосовую активацию проверяйте отдельно."
            );
        }
        _ => {
            println!("выбрано: {}", data["selected"].as_str().unwrap());
            if data["applied"] == true {
                println!("модель применена, quasar запущен");
            } else if command == "set" {
                println!("для применения выполните wakeword apply");
            }
        }
    }
}

const USAGE: &str = "wakeword [--json] [--request-id ID] <команда>\n\n  list                доступные имена\n  current             выбранное имя (не подтверждение применения)\n  status              версии файлов и CRC32\n  set <имя>           сохранить выбор\n  apply               применить сохранённый выбор, перезапустить quasar\n  switch <имя>        выбрать и применить за один вызов\n  version             версия программы\n\nИмена: alisa, yandex, yasmina. JSON API v1: --json.\nLegacy-команды add/remove сохранены; обучение своих слов не реализовано.";

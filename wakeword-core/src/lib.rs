use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const CONFIG_PATH: &str = "/data/data/wakeword/config.json";
const SPOTTER_JSON_PATH: &str = "/data/quasar/data/spotter.json";
const MODELS_DIR: &str = "/system/vendor/quasar/spotter_models/activation";
const BACKUP_DIR: &str = "/data/data/wakeword/models/builtin";
const SPOTTER_MODEL_DIR: &str = "/data/quasar/data/spotter_model";
const CACHE_BACKUP_DIR: &str = "/data/data/wakeword/models/cache-originals";
const CHECKSUM_FILE: &str = "zz-wakeword-crc32";
const QUASAR_CONFIG: &str = "/system/vendor/quasar/quasar.cfg";
const VALIDATION_STATE: &str = "/data/data/wakeword/validation-backup.json";
const QUASAR_CONFIG_BACKUP: &str = "/data/data/wakeword/quasar.cfg.before-validation";
const BUILTIN: &[&str] = &["alisa", "yandex", "yasmina"];

fn model_dir(name: &str) -> String {
    format!("{}/{}", MODELS_DIR, name)
}

fn backup_dir(name: &str) -> String {
    format!("{}/{}", BACKUP_DIR, name)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomWakeWord {
    pub name: String,
    pub model_path: String,
}

/// File diagnostics, not a claim that the voice engine loaded these files.
#[derive(Serialize, Clone, Debug)]
pub struct ModelStatus {
    pub path: String,
    pub version: String,
    pub expected_crc32: Option<u32>,
    pub actual_crc32: Option<u32>,
    pub checksum_matches: Option<bool>,
}

#[derive(Serialize, Clone, Debug)]
pub struct StatusReport {
    pub selected: String,
    pub server_activation_validation_disabled: bool,
    pub models: Vec<ModelStatus>,
    pub verification: &'static str,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WakeWordConfig {
    pub active: String,
    pub names: Vec<String>,
    pub custom: Vec<CustomWakeWord>,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            active: BUILTIN[0].to_string(),
            names: BUILTIN.iter().map(|s| s.to_string()).collect(),
            custom: Vec::new(),
        }
    }
}

pub struct Manager {
    path: String,
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Manager {
    pub fn new() -> Self {
        Self {
            path: CONFIG_PATH.to_string(),
        }
    }

    pub fn with_path(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<WakeWordConfig, Box<dyn std::error::Error>> {
        if !Path::new(&self.path).exists() {
            return Ok(WakeWordConfig {
                active: BUILTIN[0].to_string(),
                names: BUILTIN.iter().map(|s| s.to_string()).collect(),
                custom: Vec::new(),
            });
        }
        let data = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, cfg: &WakeWordConfig) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = Path::new(&self.path).parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(cfg)?;
        let staged = Path::new(&self.path).with_extension("json.new");
        fs::write(&staged, json)?;
        fs::rename(staged, &self.path)?;
        Ok(())
    }

    pub fn list_names(cfg: &WakeWordConfig) -> Vec<String> {
        cfg.names
            .iter()
            .cloned()
            .chain(cfg.custom.iter().map(|c| c.name.clone()))
            .collect()
    }

    pub fn resolve_model_path(cfg: &WakeWordConfig, name: &str) -> Option<String> {
        if let Some(p) = cfg
            .custom
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.model_path.clone())
        {
            return Some(p);
        }
        if BUILTIN.contains(&name) {
            return Some(model_dir(name));
        }
        None
    }

    /// The firmware owns spotter.json. It is a model cache index, not a list
    /// of custom wake words; overwriting it loses URLs, checksums and paths.
    pub fn activation_cache_paths() -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        if !Path::new(SPOTTER_JSON_PATH).exists() {
            return Ok(Vec::new());
        }
        activation_paths(
            &fs::read_to_string(SPOTTER_JSON_PATH)?,
            Path::new(SPOTTER_MODEL_DIR),
        )
    }

    pub fn inspect(&self) -> Result<StatusReport, Box<dyn std::error::Error>> {
        let cfg = self.load()?;
        let quasar: serde_json::Value = serde_json::from_str(&fs::read_to_string(QUASAR_CONFIG)?)?;
        let mut paths = vec![PathBuf::from(model_dir("alisa"))];
        paths.extend(Self::activation_cache_paths()?);
        let mut models = Vec::new();
        for path in paths {
            let flags = fs::read_to_string(path.join("flags.txt"))?;
            let version = flags
                .lines()
                .find_map(|line| line.strip_prefix("version "))
                .unwrap_or("unknown")
                .to_string();
            let (expected, actual) = if path.starts_with(SPOTTER_MODEL_DIR) {
                (Some(cache_checksum(&path)?), Some(model_checksum(&path)?))
            } else {
                (None, None)
            };
            models.push(ModelStatus {
                path: path.display().to_string(),
                version,
                expected_crc32: expected,
                actual_crc32: actual,
                checksum_matches: expected.zip(actual).map(|(a, b)| a == b),
            });
        }
        Ok(StatusReport {
            selected: cfg.active,
            server_activation_validation_disabled: activation_validation_disabled(&quasar)?,
            models,
            verification: "files_only",
        })
    }

    pub fn status(&self) -> Result<(), Box<dyn std::error::Error>> {
        let status = self.inspect()?;
        println!("выбрано в плагине: {}", status.selected);
        println!(
            "серверная проверка активации: {}",
            if status.server_activation_validation_disabled {
                "отключена"
            } else {
                "включена"
            }
        );
        for model in status.models {
            println!("{}: {}", model.path, model.version);
            if let (Some(actual), Some(expected)) = (model.actual_crc32, model.expected_crc32) {
                println!(
                    "  CRC32: {} / {} ({})",
                    actual,
                    expected,
                    if actual == expected {
                        "OK"
                    } else {
                        "не совпадает"
                    }
                );
            }
        }
        println!("Это состояние файлов; загруженную модель проверяйте в maind.log.");
        Ok(())
    }

    /// One lock for configuration writes and speaker restarts, shared by all CLI clients.
    /// The lock is released by the OS when its File is dropped or the process exits.
    pub fn try_lock(&self) -> Result<fs::File, std::io::Error> {
        let lock_path = Path::new(&self.path).with_extension("lock");
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path)?;
        file.try_lock().map_err(std::io::Error::from)?;
        Ok(file)
    }

    /// Apply a new choice before persisting it. A failed preflight therefore
    /// leaves the previous selection intact.
    pub fn switch(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut cfg = self.load()?;
        Self::set_active(&mut cfg, name)?;
        self.apply_config(&cfg)?;
        self.save(&cfg)
    }

    pub fn set_active(cfg: &mut WakeWordConfig, name: &str) -> Result<(), String> {
        let all = Self::list_names(cfg);
        if !all.iter().any(|n| n == name) {
            return Err(format!("имя '{}' не найдено", name));
        }
        cfg.active = name.to_string();
        Ok(())
    }

    pub fn add_custom(
        cfg: &mut WakeWordConfig,
        name: &str,
        model_path: &str,
    ) -> Result<(), String> {
        if Self::list_names(cfg).iter().any(|n| n == name) {
            return Err(format!("имя '{}' уже есть", name));
        }
        cfg.custom.push(CustomWakeWord {
            name: name.to_string(),
            model_path: model_path.to_string(),
        });
        Ok(())
    }

    pub fn remove(cfg: &mut WakeWordConfig, name: &str) -> Result<(), String> {
        if BUILTIN.contains(&name) {
            return Err(format!("встроенное имя '{}' нельзя удалить", name));
        }
        if cfg.active == name {
            return Err(format!("'{}' активно — сначала смените активное имя", name));
        }
        cfg.names.retain(|n| n != name);
        cfg.custom.retain(|c| c.name != name);
        Ok(())
    }

    pub fn apply(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.apply_config(&self.load()?)
    }

    fn apply_config(&self, cfg: &WakeWordConfig) -> Result<(), Box<dyn std::error::Error>> {
        Self::set_active(&mut cfg.clone(), &cfg.active)?;
        Self::backup_builtin_models()?;
        let source = if BUILTIN.contains(&cfg.active.as_str()) {
            PathBuf::from(backup_dir(&cfg.active))
        } else {
            PathBuf::from(Self::resolve_model_path(cfg, &cfg.active).ok_or("модель не найдена")?)
        };
        validate_model(&source)?;
        let caches = Self::activation_cache_paths()?;
        if caches.is_empty() && cfg.active != "alisa" {
            return Err("в реестре нет activation/alisa: дождитесь загрузки модели колонкой и повторите apply".into());
        }

        // Prepare and back up everything BEFORE stopping the speaker. The
        // registry remains unchanged so the current server URL/CRC still resolves
        // to the same cached directory on the next start.
        let mut replacements = Vec::new();
        let builtin_dst = PathBuf::from(model_dir("alisa"));
        replacements.push(stage_model(&source, &builtin_dst)?);
        for target in caches {
            let original = Path::new(CACHE_BACKUP_DIR)
                .join(target.file_name().ok_or("нет имени папки модели")?);
            if !original.exists() {
                validate_model(&target)?;
                let staged_backup = stage_model(&target, &original)?;
                install_staged(&staged_backup)?;
            }
            let cache_source = if cfg.active == "alisa" {
                &original
            } else {
                &source
            };
            let staged = stage_model(cache_source, &target)?;
            if cfg.active != "alisa" {
                preserve_checksum(&staged.0, cache_checksum(&target)?)?;
            }
            replacements.push(staged);
        }
        let validation_config = stage_validation_config(&cfg.active)?;

        // q stop may return 1 because its last killall cannot find begemot.
        // Check the actual processes rather than treating that as success/failure.
        let _ = run_q("stop")?;
        // killall returns before the killed tasks disappear from /proc.
        for _ in 0..30 {
            if !process_running("quasar_launcher2")? && !process_running("maind")? {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if process_running("quasar_launcher2")? || process_running("maind")? {
            return Err("q stop не остановил quasar_launcher2/maind; модели не изменены".into());
        }
        let installed = (|| -> Result<(), Box<dyn std::error::Error>> {
            replacements.iter().try_for_each(install_staged)?;
            if let Some((staged, target)) = validation_config {
                fs::rename(staged, target)?;
            }
            Ok(())
        })();
        // Always attempt to bring the platform back even if a filesystem write failed.
        let restarted = run_q("start");
        installed?;
        if !restarted?.success() {
            return Err("q start вернул ошибку".into());
        }
        for (_, target) in &replacements {
            eprintln!("Модель '{}' установлена в {}", cfg.active, target.display());
        }
        // Startup uses both the fallback and the synchronized server config.
        // Do not report success before giving both a chance to replace the cache.
        for _ in 0..5 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if !process_running("quasar_launcher2")? || !process_running("maind")? {
                return Err("после q start процессы платформы не запущены".into());
            }
            for path in Self::activation_cache_paths()? {
                if !replacements.iter().any(|(_, target)| target == &path) {
                    return Err("прошивка скачала новую модель после запуска; подмена не удержалась, проверьте maind.log".into());
                }
                if model_checksum(&path)? != cache_checksum(&path)? {
                    return Err("CRC32 модели после запуска не совпадает с реестром".into());
                }
                if cfg.active != "alisa"
                    && fs::read(path.join("flags.txt"))? != fs::read(source.join("flags.txt"))?
                {
                    return Err("после запуска вернулась другая модель; проверьте maind.log".into());
                }
            }
        }
        Ok(())
    }

    fn backup_builtin_models() -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(BACKUP_DIR)?;
        for name in BUILTIN {
            let src = PathBuf::from(model_dir(name));
            let dst = PathBuf::from(backup_dir(name));
            if src.is_dir() && !dst.exists() {
                install_staged(&stage_model(&src, &dst)?)?;
            }
        }
        Ok(())
    }
}

fn q_command() -> &'static str {
    if Path::new("/usr/bin/q").exists() {
        "/usr/bin/q"
    } else {
        "q"
    }
}

fn run_q(action: &str) -> std::io::Result<std::process::ExitStatus> {
    let output = Command::new(q_command()).arg(action).output()?;
    // Machine-readable callers must receive exactly one JSON document on stdout.
    eprint!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.status)
}

#[derive(Serialize, Deserialize)]
struct ValidationBackup {
    activation_was_disabled: bool,
}

fn activation_validation_disabled(
    config: &serde_json::Value,
) -> Result<bool, Box<dyn std::error::Error>> {
    match config.pointer("/aliced/voiceDialogSettings/onlineSpotterValidationDisabledTypes") {
        None => Ok(false),
        Some(types) => Ok(types
            .as_array()
            .ok_or("onlineSpotterValidationDisabledTypes должен быть массивом")?
            .iter()
            .any(|value| value == "activation")),
    }
}

fn set_activation_validation_disabled(
    config: &mut serde_json::Value,
    disabled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings = config
        .pointer_mut("/aliced/voiceDialogSettings")
        .and_then(|value| value.as_object_mut())
        .ok_or("в quasar.cfg нет aliced.voiceDialogSettings")?;
    let types = settings
        .entry("onlineSpotterValidationDisabledTypes")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or("onlineSpotterValidationDisabledTypes должен быть массивом")?;
    types.retain(|value| value != "activation");
    if disabled {
        types.push(serde_json::json!("activation"));
    }
    Ok(())
}

fn stage_validation_config(
    active: &str,
) -> Result<Option<(PathBuf, PathBuf)>, Box<dyn std::error::Error>> {
    if active == "alisa" && !Path::new(VALIDATION_STATE).exists() {
        return Ok(None);
    }
    let config_path = Path::new(QUASAR_CONFIG);
    let text = fs::read_to_string(config_path)?;
    let mut config: serde_json::Value = serde_json::from_str(&text)?;
    let state = if Path::new(VALIDATION_STATE).exists() {
        serde_json::from_str::<ValidationBackup>(&fs::read_to_string(VALIDATION_STATE)?)?
    } else {
        let state = ValidationBackup {
            activation_was_disabled: activation_validation_disabled(&config)?,
        };
        if !Path::new(QUASAR_CONFIG_BACKUP).exists() {
            fs::copy(config_path, QUASAR_CONFIG_BACKUP)?;
        }
        fs::write(VALIDATION_STATE, serde_json::to_string_pretty(&state)?)?;
        state
    };
    let disabled = active != "alisa" || state.activation_was_disabled;
    if activation_validation_disabled(&config)? == disabled {
        return Ok(None);
    }
    set_activation_validation_disabled(&mut config, disabled)?;
    let staged = config_path.with_file_name("quasar.cfg.wakeword-new");
    fs::write(&staged, serde_json::to_string_pretty(&config)?)?;
    fs::set_permissions(&staged, fs::metadata(config_path)?.permissions())?;
    Ok(Some((staged, config_path.to_path_buf())))
}

fn process_running(name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let output = Command::new("/bin/pidof").arg(name).output()?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!("не удалось проверить процесс {}", name).into()),
    }
}

fn activation_paths(json: &str, root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let registry: serde_json::Value = serde_json::from_str(json)?;
    let models = registry
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or("spotter.json: ожидается объект с массивом models")?;
    let mut paths = Vec::new();
    for model in models {
        if model["category"] != "spotter" || model["type"] != "activation/alisa" {
            continue;
        }
        let path = PathBuf::from(
            model["path"]
                .as_str()
                .ok_or("нет path у модели activation/alisa")?,
        );
        // Only direct cache children may be replaced. Never trust registry paths
        // as arbitrary write targets, including traversal or symlink paths.
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "модель вне каталога кэша")?;
        let mut components = relative.components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err("недопустимый путь модели в кэше".into());
        }
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("путь модели в кэше — симлинк".into());
        }
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn cache_checksum(path: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(SPOTTER_JSON_PATH)?)?;
    let models = registry["models"]
        .as_array()
        .ok_or("неверный реестр моделей")?;
    let model = models
        .iter()
        .find(|model| {
            model["path"]
                .as_str()
                .is_some_and(|value| Path::new(value) == path)
        })
        .ok_or("модель исчезла из реестра")?;
    let crc = &model["crc"];
    if let Some(value) = crc.as_str() {
        return Ok(value.parse()?);
    }
    Ok(u32::try_from(crc.as_u64().ok_or("неверный CRC32 модели")?)?)
}

fn crc32(crc: u32, bytes: &[u8]) -> u32 {
    let mut state = !crc;
    for byte in bytes {
        state ^= u32::from(*byte);
        for _ in 0..8 {
            state = (state >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(state & 1));
        }
    }
    !state
}

fn model_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            files.extend(model_files(&entry.path())?);
        } else if kind.is_file() {
            files.push(entry.path());
        } else {
            return Err("симлинки в модели не поддерживаются".into());
        }
    }
    files.sort();
    Ok(files)
}

// Firmware CRC32 is over the concatenated contents of all recursively listed
// files in lexicographic order. Filenames themselves are not hashed. Verified
// against downloaded i8, i16 and command models on Mini 2.
fn model_checksum(root: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    let mut crc = 0;
    let mut buffer = [0; 8192];
    for path in model_files(root)? {
        let mut file = fs::File::open(path)?;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            crc = crc32(crc, &buffer[..count]);
        }
    }
    Ok(crc)
}

fn checksum_suffix(prefix_crc: u32, expected: u32) -> [u8; 4] {
    let baseline = crc32(prefix_crc, &[0; 4]);
    let mut basis = [(0u32, 0u32); 32];
    for bit in 0..32 {
        let patch = 1u32 << bit;
        let mut vector = crc32(prefix_crc, &patch.to_le_bytes()) ^ baseline;
        let mut mask = patch;
        while vector != 0 {
            let pivot = vector.ilog2() as usize;
            if basis[pivot].0 == 0 {
                basis[pivot] = (vector, mask);
                break;
            }
            vector ^= basis[pivot].0;
            mask ^= basis[pivot].1;
        }
    }
    let mut wanted = expected ^ baseline;
    let mut result = 0u32;
    while wanted != 0 {
        let pivot = wanted.ilog2() as usize;
        wanted ^= basis[pivot].0;
        result ^= basis[pivot].1;
    }
    result.to_le_bytes()
}

fn preserve_checksum(root: &Path, expected: u32) -> Result<(), Box<dyn std::error::Error>> {
    let patch_path = root.join(CHECKSUM_FILE);
    if patch_path.exists() {
        fs::remove_file(&patch_path)?;
    }
    if model_files(root)?.iter().any(|path| path >= &patch_path) {
        return Err(format!("файлы модели должны сортироваться перед {}", CHECKSUM_FILE).into());
    }
    let prefix = model_checksum(root)?;
    fs::write(patch_path, checksum_suffix(prefix, expected))?;
    if model_checksum(root)? != expected {
        return Err("не удалось сохранить CRC32 модели".into());
    }
    Ok(())
}

fn validate_model(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !path.is_dir() || !path.join("flags.txt").is_file() || !path.join("words.txt").is_file() {
        return Err(format!(
            "{}: нужна папка модели с flags.txt, words.txt и весами; одного .nnet недостаточно",
            path.display()
        )
        .into());
    }
    let flags = fs::read_to_string(path.join("flags.txt"))?;
    let weights = flags
        .lines()
        .find_map(|line| line.strip_prefix("spotter-npu-model-path "))
        .unwrap_or("acoustic_model.nnet");
    let weights_path = Path::new(weights.trim());
    if weights_path.is_absolute()
        || weights_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !path.join(weights_path).is_file()
    {
        return Err(format!("{}: не найдены веса {}", path.display(), weights).into());
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(format!("неподдерживаемый тип файла: {}", entry.path().display()).into());
        }
    }
    Ok(())
}

// Staging in a sibling directory keeps renames on the same filesystem and
// prevents leftover TFLite weights/flags from mixing with the selected model.
fn stage_model(src: &Path, dst: &Path) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    validate_model(src)?;
    let stage = dst.with_file_name(format!(
        ".{}.wakeword-stage",
        dst.file_name().ok_or("нет имени папки")?.to_string_lossy()
    ));
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    copy_tree(src, &stage)?;
    validate_model(&stage)?;
    Ok((stage, dst.to_path_buf()))
}

fn install_staged((stage, dst): &(PathBuf, PathBuf)) -> Result<(), Box<dyn std::error::Error>> {
    let old = dst.with_file_name(format!(
        ".{}.wakeword-old",
        dst.file_name().ok_or("нет имени папки")?.to_string_lossy()
    ));
    if old.exists() {
        return Err(format!(
            "{}: осталась папка восстановления; проверьте её перед повторным apply",
            old.display()
        )
        .into());
    }
    let existed = dst.exists();
    if existed {
        fs::rename(dst, &old)?;
    }
    if let Err(err) = fs::rename(stage, dst) {
        if existed {
            fs::rename(&old, dst)?;
        }
        return Err(err.into());
    }
    if existed {
        fs::remove_dir_all(old)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_activation_validation_preserves_locale_and_other_types() {
        let mut config = serde_json::json!({"aliced":{"lang":"ru-RU", "voiceDialogSettings":{"onlineSpotterValidationDisabledTypes":["noise-detection","vad"]}}});
        let original = config.clone();
        assert!(!activation_validation_disabled(&config).unwrap());
        set_activation_validation_disabled(&mut config, true).unwrap();
        set_activation_validation_disabled(&mut config, true).unwrap();
        assert!(activation_validation_disabled(&config).unwrap());
        assert_eq!(
            config["aliced"]["voiceDialogSettings"]["onlineSpotterValidationDisabledTypes"],
            serde_json::json!(["noise-detection", "vad", "activation"])
        );
        set_activation_validation_disabled(&mut config, false).unwrap();
        assert_eq!(config, original);
    }

    #[test]
    fn crc32_and_compensation_match_standard_vectors() {
        assert_eq!(crc32(0, b"123456789"), 0xcbf43926);
        assert_eq!(crc32(crc32(0, b"1234"), b"56789"), 0xcbf43926);
        for prefix in [0, 0x12345678, 0xffffffff, 0xcbf43926] {
            for expected in [0, 2676544475, 4041048753, 0xffffffff] {
                assert_eq!(crc32(prefix, &checksum_suffix(prefix, expected)), expected);
            }
        }
    }

    #[test]
    fn checksum_file_preserves_weights_and_matches_firmware_crc() {
        let temp = TempDir::new();
        fs::create_dir_all(temp.0.join("tts-timings-info")).unwrap();
        fs::write(temp.0.join("acoustic_model.nnet"), "weights").unwrap();
        fs::write(temp.0.join("flags.txt"), "flags").unwrap();
        fs::write(temp.0.join("tts-timings-info/words.txt"), "timings").unwrap();
        fs::write(temp.0.join("words.txt"), "yasmina 1").unwrap();
        assert_eq!(
            model_checksum(&temp.0).unwrap(),
            crc32(0, b"weightsflagstimingsyasmina 1")
        );
        for expected in [2676544475, 4041048753] {
            preserve_checksum(&temp.0, expected).unwrap();
            assert_eq!(model_checksum(&temp.0).unwrap(), expected);
            assert_eq!(
                fs::read(temp.0.join("acoustic_model.nnet")).unwrap(),
                b"weights"
            );
            assert_eq!(fs::metadata(temp.0.join(CHECKSUM_FILE)).unwrap().len(), 4);
        }
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("wakeword-{}-{}", std::process::id(), unique));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn selects_every_alisa_cache_from_registry_without_word_matching() {
        let registry = r#"{"models":[
            {"category":"spotter","type":"activation/alisa","path":"/cache/phoneme-model"},
            {"category":"spotter","type":"activation/alisa","path":"/cache/new-model"},
            {"category":"spotter","type":"command","path":"/cache/commands"},
            {"category":"other","type":"activation/alisa","path":"/cache/other"}
        ]}"#;
        assert_eq!(
            activation_paths(registry, Path::new("/cache")).unwrap(),
            vec![
                PathBuf::from("/cache/phoneme-model"),
                PathBuf::from("/cache/new-model")
            ]
        );
    }

    #[test]
    fn rejects_wrong_registry_schema_and_unsafe_targets() {
        assert!(activation_paths("[]", Path::new("/cache")).is_err());
        for path in [
            "/cache",
            "/outside/model",
            "/cache/../outside",
            "/cache/a/nested",
        ] {
            let registry = serde_json::json!({"models":[{"category":"spotter","type":"activation/alisa","path":path}]}).to_string();
            assert!(
                activation_paths(&registry, Path::new("/cache")).is_err(),
                "{}",
                path
            );
        }
    }

    #[test]
    fn replacing_a_model_copies_nested_files_and_removes_old_weights() {
        let temp = TempDir::new();
        let source = temp.0.join("yasmina");
        let target = temp.0.join("cached-alisa");
        fs::create_dir_all(source.join("tts-timings-info")).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("flags.txt"), "version yasmina\n").unwrap();
        fs::write(source.join("words.txt"), "filler 0\nyasmina 1\n").unwrap();
        fs::write(source.join("acoustic_model.nnet"), "new weights").unwrap();
        fs::write(source.join("tts-timings-info/ru.txt"), "timings").unwrap();
        fs::write(target.join("model.tflite"), "old weights").unwrap();
        let staged = stage_model(&source, &target).unwrap();
        assert!(target.join("model.tflite").exists());
        install_staged(&staged).unwrap();
        assert!(!target.join("model.tflite").exists());
        assert_eq!(
            fs::read_to_string(target.join("acoustic_model.nnet")).unwrap(),
            "new weights"
        );
        assert_eq!(
            fs::read_to_string(target.join("tts-timings-info/ru.txt")).unwrap(),
            "timings"
        );
        assert!(source.join("acoustic_model.nnet").exists());
    }

    #[test]
    fn refuses_incomplete_model_before_modifying_target() {
        let temp = TempDir::new();
        let source = temp.0.join("incomplete");
        let target = temp.0.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("flags.txt"),
            "spotter-npu-model-path model.tflite\n",
        )
        .unwrap();
        fs::write(source.join("words.txt"), "алиса 1\n").unwrap();
        assert!(stage_model(&source, &target).is_err());
        assert!(!target.exists());
        fs::write(source.join("model.tflite"), "weights").unwrap();
        assert!(validate_model(&source).is_ok());
    }

    #[test]
    fn default_contains_builtin() {
        let cfg = WakeWordConfig::default();
        assert!(cfg.names.contains(&"alisa".to_string()));
        assert!(cfg.names.contains(&"yandex".to_string()));
        assert!(cfg.names.contains(&"yasmina".to_string()));
    }
    #[test]
    fn set_active_works() {
        let mut cfg = WakeWordConfig::default();
        Manager::set_active(&mut cfg, "yandex").unwrap();
        assert_eq!(cfg.active, "yandex");
    }
    #[test]
    fn add_and_remove_custom() {
        let mut cfg = WakeWordConfig::default();
        Manager::add_custom(&mut cfg, "ясмина", "/data/data/wakeword/models/yasmin.nnet").unwrap();
        assert!(Manager::list_names(&cfg).contains(&"ясмина".to_string()));
        Manager::remove(&mut cfg, "ясмина").unwrap();
        assert!(!Manager::list_names(&cfg).contains(&"ясмина".to_string()));
    }
    #[test]
    fn cannot_remove_builtin() {
        let mut cfg = WakeWordConfig::default();
        assert!(Manager::remove(&mut cfg, "alisa").is_err());
    }
}

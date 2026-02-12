//! Build and flash helper for ESP32 app crates.
//!
//! This binary provides a small command surface for building or flashing the
//! app crates under `apps/` without requiring manual target or feature setup.
//!
//! # Overview
//!
//! - Resolves a short target name or `.rs` path to a Cargo binary
//! - Injects the Xtensa target and `-Zbuild-std=core`
//! - Adds required linker flags for ESP32 applications
//! - Uses the ESP toolchain via `rustup run esp`
//!
//! # Usage
//!
//! ```ignore
//! cargo xtask run ex-smoltcp
//! cargo xtask build qa-runner
//! cargo xtask run ex-embassy-net --debug
//! cargo xtask run ex-esp-hal -- --extra-arg
//! ```
//!
//! # Targets
//!
//! - qa-runner | qa
//! - ex-esp-hal | ex-esp-hal-async
//! - ex-smoltcp
//! - ex-embassy | ex-embassy-net
//!
//! # Notes
//!
//! - If no command is supplied, `build` is assumed.
//! - `--debug` selects a debug build (release is the default).
//! - `--` passes arguments to the target binary.
//! - `ESP_LOG`, `ESP_IDF_VERSION`, and `CARGO_TARGET_DIR` are defaulted
//!   if not set by the caller.

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const XTASK_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Operational mode for the xtask invocation.
#[derive(Clone, Copy)]
enum Mode {
    Run,
    Build,
}

/// Cargo build profile selection.
#[derive(Clone, Copy)]
enum Profile {
    Release,
    Debug,
}

/// Cargo binary entry discovered in a manifest.
struct BinInfo {
    name: String,
    path: PathBuf,
    required_features: Vec<String>,
}

/// A resolved binary target with metadata needed for the cargo invocation.
struct ResolvedBin {
    manifest_path: PathBuf,
    bin_name: Option<String>,
    required_features: Vec<String>,
    package_name: Option<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("xtask: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    if matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_usage();
        return Ok(());
    }

    let mut mode: Option<Mode> = None;
    if matches!(args[0].as_str(), "run" | "build") {
        mode = Some(match args.remove(0).as_str() {
            "run" => Mode::Run,
            "build" => Mode::Build,
            _ => Mode::Build,
        });
    }

    let mut profile = Profile::Release;
    let mut path: Option<PathBuf> = None;
    let mut pass_args: Vec<String> = Vec::new();

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "run" => mode = Some(Mode::Run),
            "build" => mode = Some(Mode::Build),
            "--debug" => profile = Profile::Debug,
            "--release" => profile = Profile::Release,
            "--" => {
                pass_args.extend(iter);
                break;
            }
            _ => {
                if path.is_none() {
                    path = Some(PathBuf::from(resolve_target_arg(&arg)?));
                } else {
                    return Err(format!("unexpected argument: {arg}").into());
                }
            }
        }
    }

    let mode = mode.unwrap_or(Mode::Build);
    let path = path.ok_or("missing <target>")?;
    let resolved = resolve_bin(&path)?;

    run_cargo(mode, profile, &resolved, &pass_args)
}

fn print_usage() {
    eprintln!(
        "Usage:\n  cargo xtask run <target> [--debug|--release] [--] [args...]\n  cargo xtask build <target> [--debug|--release]\n\nTargets:\n  qa-runner | qa\n  ex-esp-hal | ex-esp-hal-async | ex-smoltcp | ex-embassy | ex-embassy-net\n  (or a path to a .rs entry file)\n\nNotes:\n  - If no command is supplied, `build` is assumed (no flashing).\n  - Use `--` to pass args to the target binary.\n",
    );
}

fn resolve_target_arg(arg: &str) -> Result<PathBuf, Box<dyn Error>> {
    let trimmed = arg.trim_end_matches(['/', '\\']);
    let lower = trimmed.to_ascii_lowercase();

    if trimmed.ends_with(".rs") || trimmed.contains('/') || trimmed.contains('\\') {
        return Ok(PathBuf::from(trimmed));
    }

    let target = match lower.as_str() {
        "qa" | "qa-runner" | "qa-runnner" => "apps/qa-runner/qa_runner.rs",
        "ex-esp-hal" | "esp-hal" | "ex-esp-hal-integration" => {
            "apps/examples/esp_hal_integration.rs"
        }
        "ex-esp-hal-async" | "esp-hal-async" | "ex-async" => {
            "apps/examples/esp_hal_async.rs"
        }
        "ex-smoltcp" | "smoltcp" | "ex-smoltcp-echo" => "apps/examples/smoltcp_echo.rs",
        "ex-embassy" | "embassy" | "ex-embassy-net" | "embassy-net" => {
            "apps/examples/embassy_net.rs"
        }
        "apps/examples" | "examples" => "apps/examples/esp_hal_integration.rs",
        "apps/qa-runner" => "apps/qa-runner/qa_runner.rs",
        _ => {
            return Err(format!(
                "unknown target: {arg}\nUse `cargo xtask --help` to list targets."
            )
            .into())
        }
    };

    Ok(PathBuf::from(target))
}

fn resolve_bin(path: &Path) -> Result<ResolvedBin, Box<dyn Error>> {
    let cwd = env::current_dir()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let file_path = fs::canonicalize(&path)
        .map_err(|_| format!("file not found: {}", path.display()))?;

    let manifest_path = find_manifest(&file_path)?;
    let manifest_dir = manifest_path
        .parent()
        .ok_or("manifest path has no parent")?;

    let manifest_str = fs::read_to_string(&manifest_path)?;
    let manifest: toml::Value = manifest_str.parse()?;

    let package_name = manifest
        .get("package")
        .and_then(|pkg| pkg.get("name"))
        .and_then(|name| name.as_str())
        .map(|name| name.to_string());

    let mut bins = parse_bins(&manifest, manifest_dir);
    if bins.is_empty() {
        if let Some(default_bin) = default_bin(&file_path, manifest_dir, package_name.clone()) {
            bins.push(default_bin);
        }
    }

    for bin in &bins {
        if let Ok(candidate) = fs::canonicalize(&bin.path) {
            if candidate == file_path {
                return Ok(ResolvedBin {
                    manifest_path,
                    bin_name: if bin.name.is_empty() {
                        None
                    } else {
                        Some(bin.name.clone())
                    },
                    required_features: bin.required_features.clone(),
                    package_name,
                });
            }
        }
    }

    let available = bins
        .iter()
        .map(|bin| {
            if bin.name.is_empty() {
                bin.path.display().to_string()
            } else {
                format!("{} ({})", bin.name, bin.path.display())
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    Err(format!(
        "no matching bin for {} (available: {available})",
        file_path.display()
    )
    .into())
}

fn find_manifest(file_path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut current = file_path
        .parent()
        .ok_or("file path has no parent directory")?;

    loop {
        let manifest = current.join("Cargo.toml");
        if manifest.is_file() {
            return Ok(manifest);
        }

        current = match current.parent() {
            Some(parent) => parent,
            None => break,
        };
    }

    Err("unable to locate Cargo.toml for the provided path".into())
}

fn parse_bins(manifest: &toml::Value, manifest_dir: &Path) -> Vec<BinInfo> {
    let mut bins = Vec::new();
    let bin_entries = manifest
        .get("bin")
        .and_then(|bin| bin.as_array())
        .cloned()
        .unwrap_or_default();

    for bin in bin_entries {
        let name = bin
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or("")
            .to_string();

        let path = if let Some(path) = bin.get("path").and_then(|path| path.as_str()) {
            manifest_dir.join(path)
        } else if !name.is_empty() {
            manifest_dir.join("src").join("bin").join(format!("{name}.rs"))
        } else {
            continue;
        };

        let required_features = parse_required_features(bin.get("required-features"));

        bins.push(BinInfo {
            name,
            path,
            required_features,
        });
    }

    bins
}

fn parse_required_features(value: Option<&toml::Value>) -> Vec<String> {
    match value {
        Some(toml::Value::Array(entries)) => entries
            .iter()
            .filter_map(|entry| entry.as_str().map(|s| s.to_string()))
            .collect(),
        Some(toml::Value::String(single)) => vec![single.to_string()],
        _ => Vec::new(),
    }
}

fn default_bin(
    requested: &Path,
    manifest_dir: &Path,
    package_name: Option<String>,
) -> Option<BinInfo> {
    let default_path = manifest_dir.join("src").join("main.rs");
    if fs::canonicalize(&default_path).ok()? == *requested {
        Some(BinInfo {
            name: package_name.unwrap_or_default(),
            path: default_path,
            required_features: Vec::new(),
        })
    } else {
        None
    }
}

fn run_cargo(
    mode: Mode,
    profile: Profile,
    resolved: &ResolvedBin,
    pass_args: &[String],
) -> Result<(), Box<dyn Error>> {
    let mut cargo_args = Vec::new();

    match mode {
        Mode::Run => cargo_args.push("run".to_string()),
        Mode::Build => cargo_args.push("build".to_string()),
    }

    cargo_args.push("--manifest-path".to_string());
    cargo_args.push(resolved.manifest_path.display().to_string());
    cargo_args.push("--target".to_string());
    cargo_args.push("xtensa-esp32-none-elf".to_string());
    cargo_args.push("-Zbuild-std=core".to_string());

    if matches!(profile, Profile::Release) {
        cargo_args.push("--release".to_string());
    }

    if let Some(bin_name) = &resolved.bin_name {
        if !bin_name.is_empty() {
            cargo_args.push("--bin".to_string());
            cargo_args.push(bin_name.clone());
        }
    }

    if !resolved.required_features.is_empty() {
        cargo_args.push("--features".to_string());
        cargo_args.push(resolved.required_features.join(","));
    }

    if matches!(mode, Mode::Run) {
        cargo_args.push("--config".to_string());
        cargo_args.push("target.xtensa-esp32-none-elf.runner='espflash flash --monitor'".to_string());
    }

    if needs_linkall(
        resolved.package_name.as_deref(),
        resolved.bin_name.as_deref(),
        &resolved.required_features,
    ) {
        cargo_args.push("--config".to_string());
        cargo_args.push(
            "target.xtensa-esp32-none-elf.rustflags=[\"-C\",\"link-arg=-nostartfiles\",\"-C\",\"link-arg=-Wl,-Tlinkall.x\"]"
                .to_string(),
        );
    }

    if !pass_args.is_empty() {
        cargo_args.push("--".to_string());
        cargo_args.extend(pass_args.iter().cloned());
    }

    let mut command = Command::new("rustup");
    command.arg("run").arg("esp").arg("cargo");
    command.args(&cargo_args);

    if env::var_os("ESP_LOG").is_none() {
        command.env("ESP_LOG", "info");
    }
    if env::var_os("ESP_IDF_VERSION").is_none() {
        command.env("ESP_IDF_VERSION", "v5.1");
    }
    if env::var_os("CARGO_TARGET_DIR").is_none() {
        let repo_root = Path::new(XTASK_MANIFEST_DIR)
            .parent()
            .ok_or("xtask manifest directory has no parent")?;
        command.env("CARGO_TARGET_DIR", repo_root.join("target"));
    }

    println!("xtask: rustup run esp cargo {}", cargo_args.join(" "));

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo failed (status: {status:?})").into())
    }
}

fn needs_linkall(
    package_name: Option<&str>,
    bin_name: Option<&str>,
    required_features: &[String],
) -> bool {
    if matches!(package_name, Some("ph-esp32-mac-qa-runner")) {
        return true;
    }

    if matches!(package_name, Some("ph-esp32-mac-examples")) {
        if required_features.iter().any(|feat| {
            matches!(
                feat.as_str(),
                "embassy-net-example"
                    | "esp-hal-async-example"
                    | "esp-hal-example"
                    | "smoltcp-example"
            )
        }) {
            return true;
        }

        if matches!(
            bin_name,
            Some("embassy_net" | "esp_hal_async" | "esp_hal_integration" | "smoltcp_echo")
        ) {
            return true;
        }
    }

    false
}

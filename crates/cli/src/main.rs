use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};

const PROTOCOL_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    protocol_version: u32,
    source: Option<String>,
    project_dir: Option<PathBuf>,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    check: bool,
    #[serde(default)]
    allow_unstaged: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Finished<'a> {
    protocol_version: u32,
    event: &'a str,
    exit_code: i32,
    result: ResultBody,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultBody {
    schema_version: u32,
    diagnostics: Vec<systemeame_core::Diagnostic>,
    proposed_edits: usize,
    selected_files: usize,
    changed_files: usize,
}

fn main() {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        fail(&format!("stdin read failure: {error}"));
        return;
    }
    let request: Request = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            fail(&format!("invalid protocol request: {error}"));
            return;
        }
    };
    if request.protocol_version != PROTOCOL_VERSION {
        fail("unsupported protocol version");
        return;
    }
    let run = if let Some(source) = request.source {
        analyze_inline(source)
    } else {
        analyze_project(&request)
    };
    let (diagnostics, proposed_edits, selected_files, changed_files, exit_code) = match run {
        Ok(result) => result,
        Err(message) => {
            fail(&message);
            return;
        }
    };
    let event = Finished {
        protocol_version: PROTOCOL_VERSION,
        event: "finished",
        exit_code,
        result: ResultBody {
            schema_version: 1,
            proposed_edits,
            diagnostics,
            selected_files,
            changed_files,
        },
    };
    println!(
        "{}",
        serde_json::to_string(&event).expect("serializable protocol response")
    );
    std::process::exit(exit_code);
}

fn fail(message: &str) {
    eprintln!("systemeame native error: {message}");
    std::process::exit(2);
}

type RunResult = Result<(Vec<systemeame_core::Diagnostic>, usize, usize, usize, i32), String>;

fn analyze_inline(source: String) -> RunResult {
    let analysis = systemeame_core::analyze(&source);
    let exit = i32::from(!analysis.diagnostics.is_empty());
    Ok((analysis.diagnostics, analysis.edits.len(), 1, 0, exit))
}

fn analyze_project(request: &Request) -> RunResult {
    let start = request
        .project_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let project = find_project(&start)
        .ok_or_else(|| "Could not find sfdx-project.json from --project-dir.".to_owned())?;
    let package_dirs = package_directories(&project)?;
    let files = if request.all {
        walk_packages(&package_dirs)?
    } else {
        staged_files(&project, &package_dirs)?
    };
    if !request.all && !request.allow_unstaged {
        for path in &files {
            reject_unstaged(&project, path)?;
        }
    }
    let selected_files = files.len();
    let mut diagnostics = Vec::new();
    let mut proposed = 0;
    let mut changed = 0;
    for path in files {
        let bytes = fs::read(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(error) => {
                diagnostics.push(systemeame_core::Diagnostic {
                    code: "INVALID_UTF8",
                    severity: systemeame_core::Severity::Error,
                    start_byte: error.utf8_error().valid_up_to(),
                    end_byte: error.utf8_error().valid_up_to(),
                    message: format!(
                        "{} is not valid UTF-8 and was left unchanged.",
                        display_path(&project, &path)
                    ),
                    suggestion: Some(
                        "Convert the Apex source to UTF-8, or exclude this file until it can be reviewed."
                            .to_owned(),
                    ),
                });
                continue;
            }
        };
        let mut analysis = systemeame_core::analyze(&source);
        let relative_path = display_path(&project, &path);
        for diagnostic in &mut analysis.diagnostics {
            diagnostic.message = format!("{relative_path}: {}", diagnostic.message);
        }
        proposed += analysis.edits.len();
        diagnostics.extend(analysis.diagnostics);
        if !analysis.edits.is_empty() && !request.dry_run && !request.check {
            let fixed =
                systemeame_core::apply_edits(&source, &analysis.edits).map_err(str::to_owned)?;
            fs::write(&path, fixed)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
            changed += 1;
        }
    }
    let exit = i32::from(!diagnostics.is_empty() || request.check && proposed > 0);
    Ok((diagnostics, proposed, selected_files, changed, exit))
}

fn display_path(project: &Path, path: &Path) -> String {
    path.strip_prefix(project)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn find_project(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().ok()?;
    loop {
        if current.join("sfdx-project.json").is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn package_directories(project: &Path) -> Result<Vec<PathBuf>, String> {
    let descriptor =
        fs::read_to_string(project.join("sfdx-project.json")).map_err(|error| error.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&descriptor)
        .map_err(|error| format!("Invalid sfdx-project.json: {error}"))?;
    let entries = value
        .get("packageDirectories")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "sfdx-project.json has no packageDirectories array.".to_owned())?;
    entries
        .iter()
        .filter_map(|entry| entry.get("path")?.as_str())
        .map(|relative| {
            project
                .join(relative)
                .canonicalize()
                .map_err(|error| format!("Invalid package directory {relative}: {error}"))
        })
        .collect()
}

fn staged_files(project: &Path, packages: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args([
            "-C",
            project.to_str().ok_or("Project path is not UTF-8")?,
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ])
        .output()
        .map_err(|error| format!("Git is required for staged scope: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .filter_map(|name| std::str::from_utf8(name).ok())
        .map(|name| project.join(name))
        .filter(|path| {
            apex(path) && path.is_file() && packages.iter().any(|package| path.starts_with(package))
        })
        .collect::<Vec<_>>()
        .pipe(Ok)
}

fn walk_packages(packages: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for package in packages {
        walk(package, &mut files)?;
    }
    Ok(files)
}
fn walk(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk(&entry.path(), files)?;
        } else if kind.is_file() && apex(&entry.path()) {
            files.push(entry.path());
        }
    }
    Ok(())
}
fn apex(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("cls") | Some("trigger")
    )
}

fn reject_unstaged(project: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(project)
        .map_err(|_| "Selected path escaped project root")?;
    let status = Command::new("git")
        .args([
            "-C",
            project.to_str().ok_or("Project path is not UTF-8")?,
            "diff",
            "--quiet",
            "--",
            relative.to_str().ok_or("Apex path is not UTF-8")?,
        ])
        .status()
        .map_err(|error| error.to_string())?;
    if status.code() == Some(1) {
        return Err(format!(
            "PARTIAL_STAGE: {} has unstaged changes; rerun with --allow-unstaged.",
            relative.display()
        ));
    }
    if !status.success() {
        return Err(format!("Git could not inspect {}.", relative.display()));
    }
    Ok(())
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

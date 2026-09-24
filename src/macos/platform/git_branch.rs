use objc2::rc::autoreleasepool;
use objc2_app_kit::NSRunningApplication;
use std::path::{Path, PathBuf};
use std::process::Command;
use winlane::features::git_branch::{Branches, build_branches};
use winlane::features::projects::{Kind, Project};
use winlane::{tr, trf};

const VSCODE_BUNDLE: &str = "com.microsoft.VSCode";

/// Read the branches of the repository behind the current VS Code window.
///
/// `pid`/`window_id` describe the app that was frontmost before Winlane opened;
/// `projects` is the recency-ordered VS Code project cache used as a fallback.
/// Runs on a worker thread because both AX and git calls block.
pub fn load(pid: i32, window_id: Option<u64>, projects: Vec<Project>) -> Branches {
    let repo = match resolve_repo(pid, window_id, &projects) {
        Ok(repo) => repo,
        Err(error) => {
            return Branches {
                items: Vec::new(),
                error: Some(error),
                repo: None,
            };
        }
    };
    match read_branches(&repo) {
        Ok((refs, reflog)) => Branches {
            items: build_branches(&refs, &reflog),
            error: None,
            repo: Some(repo),
        },
        Err(error) => Branches {
            items: Vec::new(),
            error: Some(error),
            repo: Some(repo),
        },
    }
}

/// Switch the repository to `branch`. Git refuses on a dirty tree or a branch
/// held by another worktree; its message is surfaced unchanged.
pub fn checkout(repo: &Path, branch: &str) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("switch")
        .arg(branch)
        .output()
        .map_err(|error| trf!("无法运行 git：{}", "Could not run git: {}", error))?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if message.is_empty() {
        trf!("无法切换到分支 {}", "Could not switch to {}", branch)
    } else {
        message
    })
}

fn resolve_repo(pid: i32, window_id: Option<u64>, projects: &[Project]) -> Result<PathBuf, String> {
    let candidate = frontmost_document(pid, window_id)
        .or_else(|| project_root(projects))
        .ok_or_else(|| {
            tr!(
                "请在 VS Code 中打开一个项目后重试。",
                "Open a project in VS Code, then try again."
            )
            .to_owned()
        })?;
    git_toplevel(&candidate).ok_or_else(|| {
        tr!(
            "当前 VS Code 项目不是 Git 仓库。",
            "The current VS Code project is not a Git repository."
        )
        .to_owned()
    })
}

/// The folder of the file open in the focused VS Code window, when available.
fn frontmost_document(pid: i32, window_id: Option<u64>) -> Option<PathBuf> {
    if !is_vscode(pid) {
        return None;
    }
    let windows = crate::macos::platform::accessibility::project_windows(pid);
    let focused = window_id.and_then(|id| windows.iter().find(|window| window.id == id))?;
    let document = focused.document.as_deref()?;
    let path = url::Url::parse(document).ok()?.to_file_path().ok()?;
    if path.is_dir() {
        Some(path)
    } else {
        path.parent().map(Path::to_path_buf)
    }
}

/// The most recently opened VS Code project, used when no focused document is known.
fn project_root(projects: &[Project]) -> Option<PathBuf> {
    let project = projects.first()?;
    match project.kind {
        Kind::Folder => Some(project.path.clone()),
        Kind::Workspace => project.path.parent().map(Path::to_path_buf),
    }
}

fn is_vscode(pid: i32) -> bool {
    autoreleasepool(|_| {
        NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            .and_then(|app| app.bundleIdentifier())
            .is_some_and(|id| id.to_string() == VSCODE_BUNDLE)
    })
}

fn git_toplevel(dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn read_branches(repo: &Path) -> Result<(String, String), String> {
    let refs = run_git(
        repo,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)\t%(committerdate:relative)\t%(HEAD)\t%(worktreepath)",
            "refs/heads",
        ],
    )?;
    // A brand-new repository has no reflog; ordering simply falls back to commit date.
    let reflog = run_git(repo, &["reflog", "--max-count=200", "--format=%gs"]).unwrap_or_default();
    Ok((refs, reflog))
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| trf!("无法运行 git：{}", "Could not run git: {}", error))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

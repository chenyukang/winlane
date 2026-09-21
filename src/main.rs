#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn launched_from_codex() -> bool {
    matches!(
        std::env::var("CODEX_INTERNAL_ORIGINATOR_OVERRIDE").as_deref(),
        Ok("Codex")
    ) || (std::env::var_os("CODEX_SHELL").is_some()
        && (std::env::var_os("CODEX_THREAD_ID").is_some()
            || std::env::var_os("CODEX_SESSION_ID").is_some()))
}

#[cfg(target_os = "macos")]
fn is_transient_codex_variable(name: &str) -> bool {
    name.starts_with("CODEX_") || matches!(name, "NO_COLOR" | "TERM" | "COLORTERM")
}

#[cfg(target_os = "macos")]
fn sanitize_codex_launch_environment() {
    if !launched_from_codex() {
        return;
    }
    let variables = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| is_transient_codex_variable(&name.to_string_lossy()))
        .collect::<Vec<_>>();
    for name in variables {
        // SAFETY: This runs at the beginning of main, before Winlane starts worker threads.
        unsafe { std::env::remove_var(name) };
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        sanitize_codex_launch_environment();
        macos::app::run();
    }
    #[cfg(not(target_os = "macos"))]
    eprintln!("Winlane requires macOS 14 or later.");
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::is_transient_codex_variable;

    #[test]
    fn only_codex_and_terminal_context_is_transient() {
        for name in [
            "CODEX_SHELL",
            "CODEX_THREAD_ID",
            "NO_COLOR",
            "TERM",
            "COLORTERM",
        ] {
            assert!(is_transient_codex_variable(name), "{name}");
        }
        for name in ["PATH", "HOME", "SHELL", "NO_COLOUR", "CODEX"] {
            assert!(!is_transient_codex_variable(name), "{name}");
        }
    }
}

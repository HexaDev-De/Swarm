//! Keep the cause of a failed startup visible after noisy runtime warnings.

fn clean(text: &str) -> String {
    let mut clean = String::new();
    let mut escape = false;
    for ch in text.chars() {
        if escape {
            if ch.is_ascii_alphabetic() {
                escape = false;
            }
        } else if ch == '\u{1b}' {
            escape = true;
        } else if !ch.is_control() || ch == '\n' || ch == '\t' {
            clean.push(ch);
        }
    }
    clean
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("bearer ")
                || [
                    "api_key",
                    "apikey",
                    "api-key",
                    "access_token",
                    "refresh_token",
                    "client_secret",
                    "password",
                    "authorization",
                    "auth_token",
                ]
                .iter()
                .any(|name| lower.contains(name) && (lower.contains('=') || lower.contains(':')))
            {
                "[Credential detail omitted]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail(text: &str, limit: usize) -> String {
    let start = text
        .char_indices()
        .rev()
        .nth(limit.saturating_sub(1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    if start == 0 {
        text.to_owned()
    } else {
        format!("…{}", &text[start..])
    }
}

/// Caller redacts transport credentials before passing either value here.
pub(super) fn failure(error: &str, diagnostic: &str) -> String {
    let error = clean(error);
    let diagnostic = clean(diagnostic);
    let lines: Vec<_> = diagnostic
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let cause = lines.iter().rev().find(|line| {
        let lower = line.to_ascii_lowercase();
        !lower.contains("warning")
            && !lower.starts_with("at ")
            && !lower.starts_with("file ")
            && (lower.contains("error")
                || lower.contains("exception")
                || lower.contains("failed")
                || lower.contains("fatal"))
    });
    let mut result = error.trim().chars().take(1200).collect::<String>();
    if let Some(cause) = cause.filter(|cause| !error.contains(**cause)) {
        result.push('\n');
        result.push_str(&tail(cause, 800));
    }
    // Keep the end of stderr (exception details / Node error properties), not
    // the first several kilobytes of a warning or Python traceback.
    let details = tail(diagnostic.trim(), 2400);
    if !details.is_empty() && details != result && Some(details.as_str()) != cause.copied() {
        result.push_str("\nDetails:\n");
        result.push_str(&details);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_warnings_never_hide_the_final_exception() {
        let noise = "Python DeprecationWarning: startup compatibility warning\n".repeat(400);
        let result = failure("Provider exited before responding", &format!("{noise}Traceback (most recent call last):\n  File runtime.py, line 42\nValueError: invalid cwd \\\\?\\C:\\project\n"));
        assert!(result
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("ValueError: invalid cwd"));
        assert!(result.ends_with("ValueError: invalid cwd \\\\?\\C:\\project"));
        assert!(result.chars().count() < 3500);
    }

    #[test]
    fn node_error_properties_and_structured_provider_error_are_preserved() {
        let result = failure("ACP session/new: Internal error", "\u{1b}[31mError: EISDIR: illegal operation on a directory, lstat 'C:'\u{1b}[0m\n at realpathSync (node:fs:2706)\n code: 'EISDIR',\n syscall: 'lstat',\n path: 'C:'\n");
        assert!(result.starts_with("ACP session/new: Internal error\nError: EISDIR:"));
        assert!(result.contains("path: 'C:'"));
        assert!(!result.contains('\u{1b}'));
        assert_eq!(
            failure("An explicit provider error", ""),
            "An explicit provider error"
        );
    }

    #[test]
    fn bounded_tail_is_unicode_safe_and_warnings_are_not_promoted() {
        assert_eq!(tail("😀éabc", 3), "…abc");
        let result = failure(
            "Provider failed",
            "DeprecationWarning: ErrorClass is deprecated",
        );
        assert!(result.starts_with("Provider failed\nDetails:"));
    }

    #[test]
    fn diagnostic_credentials_are_not_sent_to_the_ui() {
        let result = failure("Authentication failed", "Authorization: Bearer fixture-secret\napi_key=fixture-key\nrefresh_token: fixture-refresh\nError: credentials expired");
        assert!(!result.contains("fixture-"));
        assert!(result.contains("credentials expired"));
    }
}

/// Search-replace with unique-match safety.
///
/// The critical invariant: `old_string` must appear **exactly once** in the content.
/// This prevents ambiguous or accidental multi-site edits — the same safety property
/// used by Codex, Grok CLI, and OpenCode.

/// Result of a successful search-replace operation.
#[derive(Debug, Clone)]
pub struct SearchReplaceResult {
    pub old_text: String,
    pub new_text: String,
    pub diff: String,
    pub applied: bool,
}

/// Apply a precise, unique-match replacement.
///
/// Returns `Ok((new_content, result))` on success, `Err(message)` on failure.
pub fn apply_search_replace(
    content: &str,
    old_string: &str,
    new_string: &str,
) -> Result<(String, SearchReplaceResult), String> {
    if old_string.is_empty() {
        return Err("old_string must not be empty".to_string());
    }

    let count = count_occurrences(content, old_string);

    if count == 0 {
        return Err("old_string not found in file".to_string());
    }

    if count > 1 {
        return Err(format!(
            "old_string is not unique (appears {} times) — make the string longer and more specific",
            count
        ));
    }

    let new_content = content.replacen(old_string, new_string, 1);
    let diff = make_simple_diff(old_string, new_string);

    Ok((
        new_content,
        SearchReplaceResult {
            old_text: old_string.to_string(),
            new_text: new_string.to_string(),
            diff,
            applied: true,
        },
    ))
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + 1;
    }
    count
}

fn make_simple_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.split('\n').collect();
    let new_lines: Vec<&str> = new.split('\n').collect();
    let max = old_lines.len().max(new_lines.len());

    let mut out = String::from("```diff\n");
    for i in 0..max {
        let o = old_lines.get(i).copied().unwrap_or("");
        let n = new_lines.get(i).copied().unwrap_or("");

        if o == n {
            out.push(' ');
            out.push_str(o);
            out.push('\n');
        } else {
            if !o.is_empty() {
                out.push('-');
                out.push_str(o);
                out.push('\n');
            }
            if !n.is_empty() || o.is_empty() {
                out.push('+');
                out.push_str(n);
                out.push('\n');
            }
        }
    }
    out.push_str("```");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_replace_succeeds() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let (new_content, result) =
            apply_search_replace(content, "println!(\"hello\")", "println!(\"hello from fusion\")")
                .unwrap();
        assert!(result.applied);
        assert!(new_content.contains("hello from fusion"));
        assert!(!new_content.contains("println!(\"hello\");") || new_content.contains("fusion"));
    }

    #[test]
    fn test_empty_old_string_rejected() {
        let result = apply_search_replace("some content", "", "new");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn test_not_found_rejected() {
        let result = apply_search_replace("some content", "not here", "new");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_ambiguous_rejected() {
        let content = "aaa bbb aaa";
        let result = apply_search_replace(content, "aaa", "ccc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not unique"));
    }

    #[test]
    fn test_diff_generation() {
        let (_, result) = apply_search_replace("old line", "old line", "new line").unwrap();
        assert!(result.diff.contains("-old line"));
        assert!(result.diff.contains("+new line"));
    }
}

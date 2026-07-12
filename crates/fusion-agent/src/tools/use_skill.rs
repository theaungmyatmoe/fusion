use serde::Deserialize;

#[derive(Deserialize)]
struct UseSkillArgs {
    name: String,
}

pub fn execute(cwd: &str, args: &serde_json::Value) -> Result<String, String> {
    let args: UseSkillArgs = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid use_skill arguments: {}", e))?;

    let skills = fusion_core::config::load_skills(cwd);
    if let Some((_, content)) = skills.iter().find(|(name, _)| name.to_lowercase() == args.name.to_lowercase()) {
        Ok(format!(
            "--- SKILL: {} ---\n{}\n------------------",
            args.name, content
        ))
    } else {
        let available: Vec<String> = skills.into_iter().map(|(n, _)| n).collect();
        Err(format!(
            "Skill '{}' not found. Available skills: {}",
            args.name,
            if available.is_empty() { "None".to_string() } else { available.join(", ") }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_use_skill_succeeds_and_fails_appropriately() {
        let temp_dir = std::env::temp_dir().join(format!("fusion-use-skill-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let skills_dir = temp_dir.join(".agents/skills/my-cool-skill");
        fs::create_dir_all(&skills_dir).unwrap();

        let skill_file = skills_dir.join("SKILL.md");
        let skill_content = "This is a mock skill instructions file.";
        fs::write(&skill_file, skill_content).unwrap();

        let cwd = temp_dir.to_str().unwrap();

        // 1. Success case (case-insensitive name match)
        let args = serde_json::json!({
            "name": "MY-COOL-SKILL"
        });
        let result = execute(cwd, &args).unwrap();
        assert!(result.contains("MY-COOL-SKILL"));
        assert!(result.contains(skill_content));

        // 2. Failure case
        let bad_args = serde_json::json!({
            "name": "non-existent-skill"
        });
        let err = execute(cwd, &bad_args).unwrap_err();
        assert!(err.contains("Skill 'non-existent-skill' not found"));
        assert!(err.contains("my-cool-skill"));

        let _ = fs::remove_dir_all(temp_dir);
    }
}


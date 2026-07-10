use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct TasteRule {
    pub rule: String,
    pub confidence: f64,
}

/// Load taste rules from local (.fusion/taste.md) and global (~/.config/fusion/taste.md) paths.
pub fn load_taste_rules(cwd: &Path) -> Vec<TasteRule> {
    let mut rules = Vec::new();
    
    // 1. Try global taste file
    if let Some(home) = dirs::home_dir() {
        let global_paths = [
            home.join(".config").join("fusion").join("taste.md"),
            home.join(".fusion").join("taste.md"),
        ];
        for path in &global_paths {
            if path.exists() {
                if let Ok(parsed) = parse_taste_file(path) {
                    rules.extend(parsed);
                    break;
                }
            }
        }
    }
    
    // 2. Try local taste file (takes precedence / appended)
    let local_path = cwd.join(".fusion").join("taste.md");
    if local_path.exists() {
        if let Ok(parsed) = parse_taste_file(&local_path) {
            rules.extend(parsed);
        }
    }
    
    // De-duplicate rules by prioritizing later rules (local over global)
    let mut unique_rules = Vec::new();
    for rule in rules.into_iter().rev() {
        if !unique_rules.iter().any(|r: &TasteRule| r.rule == rule.rule) {
            unique_rules.push(rule);
        }
    }
    unique_rules.reverse();
    unique_rules
}

fn parse_taste_file(path: &Path) -> Result<Vec<TasteRule>, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let mut rules = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('-') || trimmed.starts_with('*') {
            let item = trimmed[1..].trim();
            // Parse rule and confidence
            // Format: "Rule description: Confidence X.XX" or "Rule description - confidence X.XX"
            let mut rule_part = item.to_string();
            let mut confidence = 1.0;
            
            if let Some(pos) = item.to_lowercase().rfind("confidence") {
                let prefix_str = item[..pos].trim_end_matches(|c| c == ':' || c == '-' || c == ' ');
                let conf_str = item[pos + "confidence".len()..].trim_start_matches(|c| c == ':' || c == '-' || c == ' ');
                if let Ok(val) = conf_str.trim().parse::<f64>() {
                    rule_part = prefix_str.to_string();
                    confidence = val;
                }
            }
            if !rule_part.is_empty() {
                rules.push(TasteRule {
                    rule: rule_part,
                    confidence,
                });
            }
        }
    }
    Ok(rules)
}

pub fn save_taste_rules(cwd: &Path, rules: &[TasteRule]) -> Result<(), std::io::Error> {
    let local_dir = cwd.join(".fusion");
    if !local_dir.exists() {
        fs::create_dir_all(&local_dir)?;
    }
    let local_path = local_dir.join("taste.md");
    let mut content = String::new();
    content.push_str("# Fusion Taste Profile\n\n");
    content.push_str("## Rules\n");
    for rule in rules {
        content.push_str(&format!("- {}: Confidence {:.2}\n", rule.rule, rule.confidence));
    }
    fs::write(local_path, content)?;
    Ok(())
}

pub fn scan_taste_preferences(cwd: &Path) -> Vec<TasteRule> {
    let mut files = Vec::new();
    walk_dir(cwd, &mut files);
    
    let mut total_single_quotes = 0;
    let mut total_double_quotes = 0;
    let mut total_with_semi = 0;
    let mut total_without_semi = 0;
    let mut total_tabs = 0;
    let mut total_spaces = 0;
    let mut space_indents = std::collections::HashMap::new();
    let mut total_snake_case = 0;
    let mut total_camel_case = 0;
    let mut total_arrow_fn = 0;
    let mut total_std_fn = 0;
    
    for file_path in &files {
        if let Ok(content) = fs::read_to_string(file_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                
                // 1. Quotes count
                total_single_quotes += trimmed.chars().filter(|&c| c == '\'').count();
                total_double_quotes += trimmed.chars().filter(|&c| c == '"').count();
                
                // 2. Semicolons count (only for statements)
                if trimmed.ends_with(';') {
                    total_with_semi += 1;
                } else if trimmed.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '"' || c == '\'' || c == ')') {
                    total_without_semi += 1;
                }
                
                // 3. Indentation
                let leading_spaces = line.len() - line.trim_start().len();
                if leading_spaces > 0 {
                    if line.starts_with('\t') {
                        total_tabs += 1;
                    } else {
                        total_spaces += 1;
                        *space_indents.entry(leading_spaces).or_insert(0) += 1;
                    }
                }
                
                // 4. Function definitions
                if trimmed.contains("=>") {
                    total_arrow_fn += 1;
                }
                if trimmed.contains("function ") || trimmed.contains("fn ") {
                    total_std_fn += 1;
                }
                
                // 5. Casing check (simple word scan)
                for word in trimmed.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                    if word.len() > 3 {
                        if word.contains('_') && word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                            total_snake_case += 1;
                        } else if word.chars().next().map_or(false, |c| c.is_ascii_lowercase()) 
                            && word.chars().any(|c| c.is_ascii_uppercase()) 
                            && word.chars().all(|c| c.is_ascii_alphanumeric()) {
                            total_camel_case += 1;
                        }
                    }
                }
            }
        }
    }
    
    let mut rules = Vec::new();
    
    // Rules computation:
    // Quotes
    let quotes_sum = total_single_quotes + total_double_quotes;
    if quotes_sum > 50 {
        let single_ratio = total_single_quotes as f64 / quotes_sum as f64;
        let double_ratio = total_double_quotes as f64 / quotes_sum as f64;
        if double_ratio > 0.60 {
            rules.push(TasteRule {
                rule: "Use double quotes for strings".to_string(),
                confidence: double_ratio,
            });
        } else if single_ratio > 0.60 {
            rules.push(TasteRule {
                rule: "Use single quotes for strings".to_string(),
                confidence: single_ratio,
            });
        }
    }
    
    // Semicolons
    let semi_sum = total_with_semi + total_without_semi;
    if semi_sum > 30 {
        let with_ratio = total_with_semi as f64 / semi_sum as f64;
        let without_ratio = total_without_semi as f64 / semi_sum as f64;
        if with_ratio > 0.65 {
            rules.push(TasteRule {
                rule: "Use semicolons at the end of statements".to_string(),
                confidence: with_ratio,
            });
        } else if without_ratio > 0.65 {
            rules.push(TasteRule {
                rule: "Avoid semicolons at the end of statements".to_string(),
                confidence: without_ratio,
            });
        }
    }
    
    // Indentation
    let indent_sum = total_tabs + total_spaces;
    if indent_sum > 20 {
        let tab_ratio = total_tabs as f64 / indent_sum as f64;
        let space_ratio = total_spaces as f64 / indent_sum as f64;
        if tab_ratio > 0.70 {
            rules.push(TasteRule {
                rule: "Indent using tabs".to_string(),
                confidence: tab_ratio,
            });
        } else if space_ratio > 0.70 {
            let best_space_size = space_indents.into_iter()
                .filter(|&(size, _)| size == 2 || size == 4 || size == 8)
                .max_by_key(|&(_, count)| count)
                .map(|(size, _)| size)
                .unwrap_or(4);
            rules.push(TasteRule {
                rule: format!("Indent using spaces (size {})", best_space_size),
                confidence: space_ratio,
            });
        }
    }
    
    // Casing
    let casing_sum = total_snake_case + total_camel_case;
    if casing_sum > 50 {
        let snake_ratio = total_snake_case as f64 / casing_sum as f64;
        let camel_ratio = total_camel_case as f64 / casing_sum as f64;
        if snake_ratio > 0.60 {
            rules.push(TasteRule {
                rule: "Prefer snake_case naming conventions".to_string(),
                confidence: snake_ratio,
            });
        } else if camel_ratio > 0.60 {
            rules.push(TasteRule {
                rule: "Prefer camelCase naming conventions".to_string(),
                confidence: camel_ratio,
            });
        }
    }
    
    // Function styles
    let fn_sum = total_arrow_fn + total_std_fn;
    if fn_sum > 10 {
        let arrow_ratio = total_arrow_fn as f64 / fn_sum as f64;
        let std_ratio = total_std_fn as f64 / fn_sum as f64;
        if arrow_ratio > 0.60 {
            rules.push(TasteRule {
                rule: "Prefer arrow functions for JS/TS".to_string(),
                confidence: arrow_ratio,
            });
        } else if std_ratio > 0.60 {
            rules.push(TasteRule {
                rule: "Prefer standard function declarations for JS/TS".to_string(),
                confidence: std_ratio,
            });
        }
    }
    
    rules
}

fn walk_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') 
                || name == "node_modules" 
                || name == "target" 
                || name == "dist" 
                || name == "build" 
                || name == "venv" 
                || name == "package-lock.json" 
                || name == "yarn.lock" 
                || name == "pnpm-lock.yaml" 
            {
                continue;
            }
            if path.is_dir() {
                walk_dir(&path, files);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "js" 
                        || ext_lower == "ts" 
                        || ext_lower == "tsx" 
                        || ext_lower == "jsx" 
                        || ext_lower == "rs" 
                        || ext_lower == "py" 
                        || ext_lower == "go" 
                        || ext_lower == "c" 
                        || ext_lower == "cpp" 
                        || ext_lower == "java" 
                    {
                        files.push(path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_taste_file() {
        let test_dir = std::env::temp_dir().join(format!("fusion_taste_test_{}", line!()));
        let _ = fs::create_dir_all(&test_dir);
        let path = test_dir.join("taste.md");
        fs::write(
            &path,
            "# Test Taste\n\n\
             - Use double quotes: Confidence 0.95\n\
             * Prefer arrow functions - confidence 0.80\n\
             - Empty rule\n",
        )
        .unwrap();

        let rules = parse_taste_file(&path).unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].rule, "Use double quotes");
        assert_eq!(rules[0].confidence, 0.95);
        assert_eq!(rules[1].rule, "Prefer arrow functions");
        assert_eq!(rules[1].confidence, 0.80);
        assert_eq!(rules[2].rule, "Empty rule");
        assert_eq!(rules[2].confidence, 1.0);
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_scan_taste_preferences() {
        let test_dir = std::env::temp_dir().join(format!("fusion_taste_scan_test_{}", line!()));
        let sub = test_dir.join("src");
        fs::create_dir_all(&sub).unwrap();

        // Write a test JS file using double quotes, semicolons, and spaces
        let mut js_content = String::new();
        js_content.push_str("function test() {\n");
        for i in 0..60 {
            js_content.push_str(&format!("  const var_{} = \"hello\";\n", i));
        }
        js_content.push_str("}\n");
        fs::write(sub.join("index.js"), js_content).unwrap();

        let rules = scan_taste_preferences(&test_dir);
        assert!(rules.iter().any(|r| r.rule == "Use double quotes for strings"));
        assert!(rules.iter().any(|r| r.rule == "Use semicolons at the end of statements"));
        assert!(rules.iter().any(|r| r.rule.starts_with("Indent using spaces")));
        let _ = fs::remove_dir_all(&test_dir);
    }
}

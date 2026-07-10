use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct DesignRule {
    pub rule: String,
    pub confidence: f64,
}

/// Load design rules from local (.fusion/design.md) and global (~/.config/fusion/design.md) paths.
pub fn load_design_rules(cwd: &Path) -> Vec<DesignRule> {
    let mut rules = Vec::new();

    // 1. Try global design file
    if let Some(home) = dirs::home_dir() {
        let global_paths = [
            home.join(".config").join("fusion").join("design.md"),
            home.join(".fusion").join("design.md"),
        ];
        for path in &global_paths {
            if path.exists() {
                if let Ok(parsed) = parse_design_file(path) {
                    rules.extend(parsed);
                    break;
                }
            }
        }
    }

    // 2. Try local design file (takes precedence / appended)
    let local_path = cwd.join(".fusion").join("design.md");
    if local_path.exists() {
        if let Ok(parsed) = parse_design_file(&local_path) {
            rules.extend(parsed);
        }
    }

    // De-duplicate rules by prioritizing later rules (local over global)
    let mut unique_rules = Vec::new();
    for rule in rules.into_iter().rev() {
        if !unique_rules.iter().any(|r: &DesignRule| r.rule == rule.rule) {
            unique_rules.push(rule);
        }
    }
    unique_rules.reverse();
    unique_rules
}

fn parse_design_file(path: &Path) -> Result<Vec<DesignRule>, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let mut rules = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('-') || trimmed.starts_with('*') {
            let item = trimmed[1..].trim();
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
                rules.push(DesignRule {
                    rule: rule_part,
                    confidence,
                });
            }
        }
    }
    Ok(rules)
}

pub fn save_design_rules(cwd: &Path, rules: &[DesignRule]) -> Result<(), std::io::Error> {
    let local_dir = cwd.join(".fusion");
    if !local_dir.exists() {
        fs::create_dir_all(&local_dir)?;
    }
    let local_path = local_dir.join("design.md");
    let mut content = String::new();
    content.push_str("# Fusion Design Profile\n\n");
    content.push_str("## Design Preferences\n");
    for rule in rules {
        content.push_str(&format!("- {}: Confidence {:.2}\n", rule.rule, rule.confidence));
    }
    fs::write(local_path, content)?;
    Ok(())
}

/// Scan the codebase for design/UI preferences.
pub fn scan_design_preferences(cwd: &Path) -> Vec<DesignRule> {
    let mut files = Vec::new();
    walk_design_files(cwd, &mut files);

    let mut rules = Vec::new();

    // Counters for CSS framework detection
    let mut tailwind_classes = 0usize;
    let mut bootstrap_classes = 0usize;
    let mut vanilla_css_files = 0usize;
    let mut css_module_files = 0usize;
    let mut styled_components = 0usize;
    let mut css_in_js = 0usize;

    // Component library detection
    let mut shadcn_imports = 0usize;
    let mut mui_imports = 0usize;
    let mut antd_imports = 0usize;
    let mut chakra_imports = 0usize;

    // Icon library detection
    let mut lucide_imports = 0usize;
    let mut heroicons_imports = 0usize;
    let mut fontawesome_imports = 0usize;
    let mut material_icons = 0usize;

    // Typography/font detection
    let mut google_fonts = Vec::<String>::new();
    let mut system_font_stack = false;

    // Animation detection
    let mut framer_motion = 0usize;
    let mut gsap_usage = 0usize;
    let mut css_animations = 0usize;
    let mut css_transitions = 0usize;

    // Layout detection
    let mut flexbox_usage = 0usize;
    let mut grid_usage = 0usize;

    // Dark mode detection
    let mut prefers_color_scheme = 0usize;
    let mut dark_class = 0usize;
    let mut css_variables = 0usize;

    // Responsive detection
    let mut media_queries = 0usize;
    let mut container_queries = 0usize;

    // Emil Kowalski Design Engineering patterns
    let mut transition_all_usages = 0usize;
    let mut scale_zero_usages = 0usize;
    let mut ease_in_usages = 0usize;
    let mut button_active_scale = 0usize;
    let mut custom_easing_curves = 0usize;

    for file_path in &files {
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Track CSS module files
        if filename.ends_with(".module.css") || filename.ends_with(".module.scss") {
            css_module_files += 1;
        } else if ext == "css" || ext == "scss" || ext == "sass" || ext == "less" {
            vanilla_css_files += 1;
        }

        if let Ok(content) = fs::read_to_string(file_path) {
            // Check button active scale over full file content (since they appear on different lines)
            if (content.contains(":active") || content.contains("active:")) && (content.contains("scale(0.9") || content.contains("scale-9") || content.contains("scale(0.8")) {
                button_active_scale += 1;
            }

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // --- Emil Kowalski UI Polish Scans ---
                if trimmed.contains("transition: all") || trimmed.contains("transition-all") || trimmed.contains("transition:all") {
                    transition_all_usages += 1;
                }
                if trimmed.contains("scale(0)") || trimmed.contains("scale-0") || trimmed.contains("scale: 0") {
                    scale_zero_usages += 1;
                }
                if trimmed.contains("ease-in") && !trimmed.contains("ease-in-out") {
                    ease_in_usages += 1;
                }
                if trimmed.contains("cubic-bezier") || trimmed.contains("easing.dev") || trimmed.contains("easings.co") {
                    custom_easing_curves += 1;
                }

                // --- CSS Framework detection ---
                // Tailwind: className with utility classes
                if trimmed.contains("className=") || trimmed.contains("class=") {
                    let tw_patterns = ["flex ", "grid ", "bg-", "text-", "p-", "m-", "w-", "h-",
                                       "rounded", "shadow", "border-", "gap-", "space-",
                                       "items-", "justify-", "hover:", "dark:", "sm:", "md:", "lg:"];
                    let tw_count = tw_patterns.iter().filter(|p| trimmed.contains(*p)).count();
                    if tw_count >= 2 {
                        tailwind_classes += 1;
                    }
                }
                // Bootstrap
                if trimmed.contains("btn-primary") || trimmed.contains("container-fluid")
                    || trimmed.contains("row") && trimmed.contains("col-")
                    || trimmed.contains("navbar-") || trimmed.contains("card-body")
                {
                    bootstrap_classes += 1;
                }
                // Styled Components / CSS-in-JS
                if trimmed.contains("styled.") || trimmed.contains("styled(") {
                    styled_components += 1;
                }
                if trimmed.contains("css`") || trimmed.contains("css({") {
                    css_in_js += 1;
                }

                // --- Component Library detection ---
                if trimmed.contains("@/components/ui/") || trimmed.contains("from \"@/components/ui") {
                    shadcn_imports += 1;
                }
                if trimmed.contains("@mui/") || trimmed.contains("from '@mui/") || trimmed.contains("from \"@mui/") {
                    mui_imports += 1;
                }
                if trimmed.contains("antd") && (trimmed.contains("import") || trimmed.contains("from")) {
                    antd_imports += 1;
                }
                if trimmed.contains("@chakra-ui/") {
                    chakra_imports += 1;
                }

                // --- Icon Library detection ---
                if trimmed.contains("lucide-react") || trimmed.contains("lucide-") {
                    lucide_imports += 1;
                }
                if trimmed.contains("@heroicons/") || trimmed.contains("heroicons") {
                    heroicons_imports += 1;
                }
                if trimmed.contains("@fortawesome/") || trimmed.contains("font-awesome") || trimmed.contains("fa-") {
                    fontawesome_imports += 1;
                }
                if trimmed.contains("@mui/icons-material") || trimmed.contains("material-icons") {
                    material_icons += 1;
                }

                // --- Typography detection ---
                if trimmed.contains("fonts.googleapis.com") || trimmed.contains("fonts.google") {
                    // Try to extract font name
                    if let Some(family_pos) = trimmed.find("family=") {
                        let after = &trimmed[family_pos + 7..];
                        let font_name: String = after.chars()
                            .take_while(|c| *c != '&' && *c != '"' && *c != '\'' && *c != ')')
                            .collect();
                        let font_name = font_name.replace('+', " ").replace("%20", " ");
                        let font_name = font_name.split(':').next().unwrap_or(&font_name).trim().to_string();
                        if !font_name.is_empty() && !google_fonts.contains(&font_name) {
                            google_fonts.push(font_name);
                        }
                    }
                }
                if trimmed.contains("system-ui") || trimmed.contains("-apple-system")
                    || trimmed.contains("BlinkMacSystemFont")
                {
                    system_font_stack = true;
                }

                // --- Animation detection ---
                if trimmed.contains("framer-motion") || trimmed.contains("motion.") || trimmed.contains("from \"motion") {
                    framer_motion += 1;
                }
                if trimmed.contains("gsap") || trimmed.contains("ScrollTrigger") {
                    gsap_usage += 1;
                }
                if trimmed.contains("@keyframes") || trimmed.contains("animation:") || trimmed.contains("animation-") {
                    css_animations += 1;
                }
                if trimmed.contains("transition:") || trimmed.contains("transition-") {
                    css_transitions += 1;
                }

                // --- Layout detection ---
                if trimmed.contains("display: flex") || trimmed.contains("display:flex")
                    || (trimmed.contains("className") && trimmed.contains("flex"))
                {
                    flexbox_usage += 1;
                }
                if trimmed.contains("display: grid") || trimmed.contains("display:grid")
                    || trimmed.contains("grid-template") || trimmed.contains("grid-cols")
                {
                    grid_usage += 1;
                }

                // --- Dark mode detection ---
                if trimmed.contains("prefers-color-scheme") {
                    prefers_color_scheme += 1;
                }
                if trimmed.contains("dark:") || trimmed.contains(".dark ") || trimmed.contains("class=\"dark") {
                    dark_class += 1;
                }
                if trimmed.contains("--") && (trimmed.contains("color") || trimmed.contains("bg") || trimmed.contains("foreground")) {
                    css_variables += 1;
                }

                // --- Responsive detection ---
                if trimmed.contains("@media") {
                    media_queries += 1;
                }
                if trimmed.contains("@container") {
                    container_queries += 1;
                }
            }
        }
    }

    // --- Generate rules from collected data ---

    // CSS Framework
    let css_sum = tailwind_classes + bootstrap_classes + styled_components + css_in_js;
    if css_sum > 5 {
        if tailwind_classes > 0 && tailwind_classes as f64 / css_sum as f64 > 0.5 {
            rules.push(DesignRule {
                rule: "Uses Tailwind CSS for styling".to_string(),
                confidence: (tailwind_classes as f64 / css_sum as f64).min(1.0),
            });
        }
        if bootstrap_classes > 0 && bootstrap_classes as f64 / css_sum as f64 > 0.5 {
            rules.push(DesignRule {
                rule: "Uses Bootstrap CSS framework".to_string(),
                confidence: (bootstrap_classes as f64 / css_sum as f64).min(1.0),
            });
        }
        if styled_components > 0 && styled_components as f64 / css_sum as f64 > 0.3 {
            rules.push(DesignRule {
                rule: "Uses Styled Components (CSS-in-JS)".to_string(),
                confidence: (styled_components as f64 / css_sum as f64).min(1.0),
            });
        }
    } else if vanilla_css_files > 0 {
        rules.push(DesignRule {
            rule: "Uses vanilla CSS for styling".to_string(),
            confidence: 0.80,
        });
    }

    if css_module_files > 2 {
        rules.push(DesignRule {
            rule: "Uses CSS Modules for scoped styling".to_string(),
            confidence: 0.90,
        });
    }

    // Component Library
    let comp_libs = [
        (shadcn_imports, "Uses shadcn/ui component library"),
        (mui_imports, "Uses Material UI (MUI) component library"),
        (antd_imports, "Uses Ant Design component library"),
        (chakra_imports, "Uses Chakra UI component library"),
    ];
    for (count, name) in &comp_libs {
        if *count > 2 {
            rules.push(DesignRule {
                rule: name.to_string(),
                confidence: (*count as f64 / (*count as f64 + 5.0)).min(0.98),
            });
        }
    }

    // Icon Library
    let icon_libs = [
        (lucide_imports, "Uses Lucide React for icons"),
        (heroicons_imports, "Uses Heroicons for icons"),
        (fontawesome_imports, "Uses Font Awesome for icons"),
        (material_icons, "Uses Material Icons"),
    ];
    for (count, name) in &icon_libs {
        if *count > 1 {
            rules.push(DesignRule {
                rule: name.to_string(),
                confidence: (*count as f64 / (*count as f64 + 3.0)).min(0.98),
            });
        }
    }

    // Typography
    if !google_fonts.is_empty() {
        let fonts_str = google_fonts.join(", ");
        rules.push(DesignRule {
            rule: format!("Uses Google Fonts: {}", fonts_str),
            confidence: 0.95,
        });
    }
    if system_font_stack {
        rules.push(DesignRule {
            rule: "Uses system font stack for native feel".to_string(),
            confidence: 0.85,
        });
    }

    // Animation
    if framer_motion > 2 {
        rules.push(DesignRule {
            rule: "Uses Framer Motion for animations".to_string(),
            confidence: (framer_motion as f64 / (framer_motion as f64 + 3.0)).min(0.98),
        });
    }
    if gsap_usage > 1 {
        rules.push(DesignRule {
            rule: "Uses GSAP for animations".to_string(),
            confidence: (gsap_usage as f64 / (gsap_usage as f64 + 3.0)).min(0.98),
        });
    }
    let anim_sum = css_animations + css_transitions;
    if anim_sum > 5 && framer_motion == 0 && gsap_usage == 0 {
        rules.push(DesignRule {
            rule: "Uses native CSS animations and transitions".to_string(),
            confidence: 0.80,
        });
    }

    // Layout
    let layout_sum = flexbox_usage + grid_usage;
    if layout_sum > 10 {
        let flex_ratio = flexbox_usage as f64 / layout_sum as f64;
        let grid_ratio = grid_usage as f64 / layout_sum as f64;
        if flex_ratio > 0.70 {
            rules.push(DesignRule {
                rule: "Primarily uses Flexbox for layouts".to_string(),
                confidence: flex_ratio,
            });
        } else if grid_ratio > 0.40 {
            rules.push(DesignRule {
                rule: "Uses CSS Grid for layouts".to_string(),
                confidence: grid_ratio,
            });
        } else {
            rules.push(DesignRule {
                rule: "Mixes Flexbox and CSS Grid for layouts".to_string(),
                confidence: 0.75,
            });
        }
    }

    // Dark Mode
    if dark_class > 3 || prefers_color_scheme > 0 {
        if dark_class > prefers_color_scheme {
            rules.push(DesignRule {
                rule: "Dark mode via class-based toggling (e.g. Tailwind dark:)".to_string(),
                confidence: 0.90,
            });
        } else {
            rules.push(DesignRule {
                rule: "Dark mode via prefers-color-scheme media query".to_string(),
                confidence: 0.85,
            });
        }
    }
    if css_variables > 10 {
        rules.push(DesignRule {
            rule: "Uses CSS custom properties (variables) for theming".to_string(),
            confidence: (css_variables as f64 / (css_variables as f64 + 10.0)).min(0.98),
        });
    }

    // Responsive
    if media_queries > 5 {
        rules.push(DesignRule {
            rule: "Uses media queries for responsive design".to_string(),
            confidence: 0.90,
        });
    }
    if container_queries > 2 {
        rules.push(DesignRule {
            rule: "Uses CSS container queries for responsive components".to_string(),
            confidence: 0.85,
        });
    }

    // --- Emil Kowalski Design Engineering Rules ---
    if transition_all_usages > 3 {
        rules.push(DesignRule {
            rule: "Warning: Codebase uses 'transition: all' (animation slop, specify exact properties instead)".to_string(),
            confidence: 0.90,
        });
    } else if transition_all_usages > 0 {
        rules.push(DesignRule {
            rule: "Avoids 'transition: all' for cleaner performance (good design engineering)".to_string(),
            confidence: 0.80,
        });
    }

    if scale_zero_usages > 2 {
        rules.push(DesignRule {
            rule: "Warning: Codebase uses scale(0) animations (nothing in the real world appears from absolute zero; animate from scale(0.95) with opacity instead)".to_string(),
            confidence: 0.90,
        });
    } else if scale_zero_usages == 0 && (framer_motion > 0 || css_animations > 0) {
        rules.push(DesignRule {
            rule: "Avoids scale(0) animations for natural entry transitions".to_string(),
            confidence: 0.85,
        });
    }

    if ease_in_usages > 3 {
        rules.push(DesignRule {
            rule: "Warning: Codebase uses 'ease-in' on enter transitions (makes interface feel slow/sluggish; use ease-out instead)".to_string(),
            confidence: 0.88,
        });
    }

    if button_active_scale > 0 {
        rules.push(DesignRule {
            rule: "Uses responsive transform scale on button press active state (good UI polish)".to_string(),
            confidence: 0.92,
        });
    }

    if custom_easing_curves > 0 {
        rules.push(DesignRule {
            rule: "Uses custom easing curves (cubic-bezier) for organic motion".to_string(),
            confidence: 0.95,
        });
    }

    rules
}

fn walk_design_files(dir: &Path, files: &mut Vec<PathBuf>) {
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
                || name == ".next"
                || name == "package-lock.json"
                || name == "yarn.lock"
                || name == "pnpm-lock.yaml"
            {
                continue;
            }
            if path.is_dir() {
                walk_design_files(&path, files);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "js"
                        || ext_lower == "ts"
                        || ext_lower == "tsx"
                        || ext_lower == "jsx"
                        || ext_lower == "css"
                        || ext_lower == "scss"
                        || ext_lower == "sass"
                        || ext_lower == "less"
                        || ext_lower == "html"
                        || ext_lower == "vue"
                        || ext_lower == "svelte"
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
    fn test_parse_design_file() {
        let test_dir = std::env::temp_dir().join(format!("fusion_design_test_{}", line!()));
        let _ = fs::create_dir_all(&test_dir);
        let path = test_dir.join("design.md");
        fs::write(
            &path,
            "# Design Profile\n\n\
             - Uses Tailwind CSS: Confidence 0.92\n\
             * Uses shadcn/ui components - confidence 0.88\n\
             - Dark mode via class toggling\n",
        )
        .unwrap();

        let rules = parse_design_file(&path).unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].rule, "Uses Tailwind CSS");
        assert_eq!(rules[0].confidence, 0.92);
        assert_eq!(rules[1].rule, "Uses shadcn/ui components");
        assert_eq!(rules[1].confidence, 0.88);
        assert_eq!(rules[2].rule, "Dark mode via class toggling");
        assert_eq!(rules[2].confidence, 1.0);
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_scan_design_tailwind() {
        let test_dir = std::env::temp_dir().join(format!("fusion_design_scan_{}", line!()));
        let sub = test_dir.join("src");
        fs::create_dir_all(&sub).unwrap();

        // Write a test TSX file with Tailwind classes
        let mut tsx_content = String::new();
        tsx_content.push_str("import { Button } from '@/components/ui/button';\n");
        tsx_content.push_str("import { Card } from '@/components/ui/card';\n");
        tsx_content.push_str("import { Input } from '@/components/ui/input';\n");
        tsx_content.push_str("import { ArrowRight } from 'lucide-react';\n");
        tsx_content.push_str("import { Check } from 'lucide-react';\n\n");
        tsx_content.push_str("export default function Page() {\n");
        for i in 0..10 {
            tsx_content.push_str(&format!(
                "  return <div className=\"flex items-center bg-slate-900 p-{} text-white rounded shadow\">\n",
                i
            ));
        }
        tsx_content.push_str("}\n");
        fs::write(sub.join("page.tsx"), tsx_content).unwrap();

        let rules = scan_design_preferences(&test_dir);
        assert!(rules.iter().any(|r| r.rule.contains("Tailwind")));
        assert!(rules.iter().any(|r| r.rule.contains("shadcn")));
        assert!(rules.iter().any(|r| r.rule.contains("Lucide")));
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_scan_design_google_fonts() {
        let test_dir = std::env::temp_dir().join(format!("fusion_design_fonts_{}", line!()));
        let sub = test_dir.join("src");
        fs::create_dir_all(&sub).unwrap();

        let html_content = r#"<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;700&display=swap" rel="stylesheet">"#;
        fs::write(sub.join("index.html"), html_content).unwrap();

        let rules = scan_design_preferences(&test_dir);
        assert!(rules.iter().any(|r| r.rule.contains("Inter")));
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_scan_design_framer_motion() {
        let test_dir = std::env::temp_dir().join(format!("fusion_design_motion_{}", line!()));
        let sub = test_dir.join("src");
        fs::create_dir_all(&sub).unwrap();

        let mut tsx = String::new();
        tsx.push_str("import { motion } from 'framer-motion';\n");
        tsx.push_str("export function Comp() {\n");
        tsx.push_str("  return <motion.div animate={{ x: 100 }} />;\n");
        tsx.push_str("  return <motion.div animate={{ y: 200 }} />;\n");
        tsx.push_str("  return <motion.div animate={{ z: 300 }} />;\n");
        tsx.push_str("}\n");
        fs::write(sub.join("comp.tsx"), tsx).unwrap();

        let rules = scan_design_preferences(&test_dir);
        assert!(rules.iter().any(|r| r.rule.contains("Framer Motion")));
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_scan_design_polish_rules() {
        let test_dir = std::env::temp_dir().join(format!("fusion_design_polish_{}", line!()));
        let sub = test_dir.join("src");
        fs::create_dir_all(&sub).unwrap();

        // Write a css file with transition: all, custom easing curves, and active button scale
        let mut css = String::new();
        css.push_str(".btn {\n");
        css.push_str("  transition: all 0.2s;\n");
        css.push_str("  transition: transform 0.2s cubic-bezier(0.23, 1, 0.32, 1);\n");
        css.push_str("}\n");
        css.push_str(".btn:active {\n");
        css.push_str("  transform: scale(0.97);\n");
        css.push_str("}\n");
        css.push_str(".popover {\n");
        css.push_str("  transform: scale(0);\n");
        css.push_str("}\n");
        // write multiple times to trigger warnings
        for i in 0..5 {
            css.push_str(&format!(".class{} {{ transition: all 0.1s; transform: scale(0); }}\n", i));
        }

        fs::write(sub.join("styles.css"), css).unwrap();

        let rules = scan_design_preferences(&test_dir);
        assert!(rules.iter().any(|r| r.rule.contains("Warning: Codebase uses 'transition: all'")));
        assert!(rules.iter().any(|r| r.rule.contains("Warning: Codebase uses scale(0)")));
        assert!(rules.iter().any(|r| r.rule.contains("custom easing curves")));
        assert!(rules.iter().any(|r| r.rule.contains("active state")));
        let _ = fs::remove_dir_all(&test_dir);
    }
}

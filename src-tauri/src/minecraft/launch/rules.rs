use crate::minecraft::dto::piston_meta::Rule;
use crate::utils::system_info::{Architecture, OperatingSystem, ARCHITECTURE, OS};
use log::info;

pub struct RuleProcessor;

impl RuleProcessor {
    // Helper function to check only the conditions of a rule
    fn check_rule_conditions(rule: &Rule) -> bool {
        let mut conditions_met = true;

        // Check OS-specific rules
        if let Some(os) = &rule.os {
            let current_os_str = match OS {
                OperatingSystem::WINDOWS => Some("windows"),
                OperatingSystem::LINUX => Some("linux"),
                OperatingSystem::OSX => Some("osx"),
                _ => None,
            };

            if current_os_str.is_none() {
                info!("    ❓ Cannot determine current OS, conditions fail");
                return false; // Unknown OS cannot satisfy OS rules
            }
            let current_os = current_os_str.unwrap();

            if let Some(name) = &os.name {
                info!("    OS check: required={}, current={}", name, current_os);
                if name != current_os {
                    info!("    ❌ OS does not match");
                    conditions_met = false;
                } else {
                    info!("    ✅ OS matches");
                }
            }

            // Only proceed with arch check if OS check passed so far
            if conditions_met {
                if let Some(arch) = &os.arch {
                    let current_arch_str = match ARCHITECTURE {
                        Architecture::X86 => Some("x86"),
                        Architecture::X64 => Some("x64"),
                        Architecture::ARM => Some("arm"),
                        Architecture::AARCH64 => Some("aarch64"),
                        _ => None,
                    };

                    if current_arch_str.is_none() {
                        info!("    ❓ Cannot determine current architecture, conditions fail");
                        return false; // Unknown arch cannot satisfy arch rules
                    }
                    let current_arch = current_arch_str.unwrap();

                    info!(
                        "    Arch check: required={}, current={}",
                        arch, current_arch
                    );
                    if arch != current_arch {
                        info!("    ❌ Architecture does not match");
                        conditions_met = false;
                    } else {
                        info!("    ✅ Architecture matches");
                    }
                }
            }
        }

        // Check features only if other conditions passed so far
        if conditions_met {
            if let Some(features) = &rule.features {
                info!("    Features check:");
                // Simplified: assume none of these features are currently supported by the launcher
                // These features, if required by a rule (set to true), will cause the condition to fail.
                let feature_disallowed = features.is_demo_user.unwrap_or(false)
                    || features.has_custom_resolution.unwrap_or(false)
                    || features.has_quick_plays_support.unwrap_or(false)
                    || features.is_quick_play_singleplayer.unwrap_or(false)
                    || features.is_quick_play_multiplayer.unwrap_or(false)
                    || features.is_quick_play_realms.unwrap_or(false);

                if feature_disallowed {
                    info!("    ❌ Feature required by rule is not supported");
                    conditions_met = false;
                } else {
                    info!("    ✅ Features match (or no specific features required/disallowed)");
                }
            }
        }

        conditions_met
    }

    pub fn should_include_library(rules: &Option<Vec<Rule>>) -> bool {
        info!("\nChecking library rules");

        if let Some(rules) = rules {
            let mut allow_found = false;
            let mut disallow_found = false; // For clarity in logs
            let mut allow_matches = false;
            let mut disallow_matches = false;

            for rule in rules {
                info!("  Rule: action={}", rule.action);
                let conditions_match = Self::check_rule_conditions(rule);

                match rule.action.as_str() {
                    "allow" => {
                        allow_found = true;
                        if conditions_match {
                            allow_matches = true;
                            info!("    ✅ Allow rule's conditions matched");
                        } else {
                            info!("    ❌ Allow rule's conditions did not match");
                        }
                    }
                    "disallow" => {
                        disallow_found = true;
                        if conditions_match {
                            disallow_matches = true;
                            info!("    ✅ Disallow rule's conditions matched");
                            // Optimization: If a disallow matches, we exclude immediately.
                            break;
                        } else {
                            info!("    ❌ Disallow rule's conditions did not match");
                        }
                    }
                    _ => {
                        info!("    ❓ Unknown rule action: {}", rule.action);
                    }
                }
            }

            let should_include = if disallow_matches {
                false // A matching disallow rule excludes
            } else if allow_found && !allow_matches {
                false // An allow rule exists, but none matched
            } else {
                // Default include: include if no matching disallow, and either an allow matched,
                // or no allow rules existed at all (only non-matching disallows perhaps).
                true
            };

            info!(
                "  Final decision: {}",
                if should_include {
                    "✅ INCLUDED"
                } else {
                    "❌ EXCLUDED"
                }
            );
            should_include
        } else {
            info!("  ✅ No rules found, including by default");
            true // Standard behavior: include if no rules apply
        }
    }

    pub fn should_apply_argument(rules: &[Rule]) -> bool {
        info!("\nChecking argument rules");

        for rule in rules {
            info!("  Rule: action={}", rule.action);
            let conditions_match = Self::check_rule_conditions(rule);

            match rule.action.as_str() {
                "allow" => {
                    if !conditions_match {
                        info!("    ❌ Allow rule conditions did not match, rejecting argument");
                        return false;
                    }
                    info!("    ✅ Allow rule conditions matched");
                }
                "disallow" => {
                    if conditions_match {
                        info!("    ❌ Disallow rule conditions matched, rejecting argument");
                        return false;
                    }
                    info!("    ✅ Disallow rule conditions did not match");
                }
                _ => {
                    info!(
                        "    ❓ Unknown rule action: {}, rejecting argument",
                        rule.action
                    );
                    return false;
                }
            }
        }

        info!("  ✅ All rules allow the argument");
        true
    }
}

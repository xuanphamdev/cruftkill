//! Advisory metadata for found cruft folders.
//!
//! This module is pure: classification depends only on the matched basename,
//! existing profile tables, and optional path-risk analysis. Delete guards in
//! `safe_delete`/`delete` remain the authority for filesystem mutations.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use crate::core::profiles::base_profiles;
use crate::core::types::RiskAnalysis;

/// User-facing metadata shown in the TUI and emitted by NDJSON mode.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CruftMetadata<'a> {
    pub target_name: Cow<'a, str>,
    pub ecosystems: &'static [&'static str],
    pub category: CruftCategory,
    pub delete_risk: DeleteRiskLevel,
    pub delete_risk_reason: &'a str,
    pub rebuild_hint: Option<&'static str>,
}

/// Broad cleanup category for a matched folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CruftCategory {
    DependencyTree,
    BuildOutput,
    TestCache,
    ToolCache,
    VirtualEnvironment,
    EditorCache,
    DeploymentCache,
    Unknown,
}

impl CruftCategory {
    pub fn as_json_label(self) -> &'static str {
        match self {
            Self::DependencyTree => "dependency-tree",
            Self::BuildOutput => "build-output",
            Self::TestCache => "test-cache",
            Self::ToolCache => "tool-cache",
            Self::VirtualEnvironment => "virtual-environment",
            Self::EditorCache => "editor-cache",
            Self::DeploymentCache => "deployment-cache",
            Self::Unknown => "unknown",
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Self::DependencyTree => "dependency tree",
            Self::BuildOutput => "build output",
            Self::TestCache => "test cache",
            Self::ToolCache => "tool cache",
            Self::VirtualEnvironment => "virtual env",
            Self::EditorCache => "editor cache",
            Self::DeploymentCache => "deployment cache",
            Self::Unknown => "unknown",
        }
    }
}

/// Advisory delete-risk level. Sensitive paths always upgrade to `High`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeleteRiskLevel {
    Low,
    Medium,
    High,
}

impl DeleteRiskLevel {
    pub fn as_json_label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn display_label(self) -> &'static str {
        self.as_json_label()
    }
}

/// Classify a found path. Uses the path basename as the target name.
pub fn classify_path<'a>(path: &'a Path, risk: Option<&'a RiskAnalysis>) -> CruftMetadata<'a> {
    let target_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| path.to_string_lossy());
    classify_target_name(target_name, risk)
}

/// Classify a target basename. Useful for tests and custom output paths.
pub fn classify_target<'a>(
    target_name: &'a str,
    risk: Option<&'a RiskAnalysis>,
) -> CruftMetadata<'a> {
    classify_target_name(Cow::Borrowed(target_name), risk)
}

fn classify_target_name<'a>(
    target_name: Cow<'a, str>,
    risk: Option<&'a RiskAnalysis>,
) -> CruftMetadata<'a> {
    let target = target_name.as_ref();
    let ecosystems = ecosystems_for_target(target);
    let category = category_for_target(target);
    let (delete_risk, delete_risk_reason) =
        delete_risk_for(target, category, ecosystems.is_empty(), risk);
    let rebuild_hint = rebuild_hint_for(target, category);

    CruftMetadata {
        target_name,
        ecosystems,
        category,
        delete_risk,
        delete_risk_reason,
        rebuild_hint,
    }
}

/// Return all profile names whose target list contains `target_name`.
pub fn ecosystems_for_target(target_name: &str) -> &'static [&'static str] {
    target_ecosystems().get(target_name).map(Vec::as_slice).unwrap_or(&[])
}

fn target_ecosystems() -> &'static BTreeMap<&'static str, Vec<&'static str>> {
    static TARGET_ECOSYSTEMS: OnceLock<BTreeMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    TARGET_ECOSYSTEMS.get_or_init(|| {
        let mut grouped: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
        for (profile_name, profile) in base_profiles() {
            for target in profile.targets {
                grouped.entry(*target).or_default().insert(*profile_name);
            }
        }

        grouped.into_iter().map(|(target, names)| (target, names.into_iter().collect())).collect()
    })
}

fn category_for_target(target_name: &str) -> CruftCategory {
    match target_name {
        "node_modules" | "deps" | ".bundle" => CruftCategory::DependencyTree,

        ".venv" | "venv" | ".tox" | ".nox" => CruftCategory::VirtualEnvironment,

        "target"
        | "out"
        | "DerivedData"
        | "obj"
        | "_build"
        | "dist-newstyle"
        | ".stack-work"
        | "CMakeFiles"
        | "cmake-build-debug"
        | "cmake-build-release"
        | ".cxx"
        | "externalNativeBuild"
        | "Library"
        | "Temp"
        | "Obj"
        | "Intermediate"
        | "DerivedDataCache"
        | "Binaries"
        | ".import"
        | ".godot"
        | "storybook-static"
        | ".next"
        | ".nuxt"
        | ".svelte-kit"
        | "gatsby_cache"
        | ".docusaurus" => CruftCategory::BuildOutput,

        "coverage" | ".nyc_output" | ".jest" | ".pytest_cache" | "htmlcov" | "TestResults"
        | "cover" | ".ipynb_checkpoints" => CruftCategory::TestCache,

        ".vs" | ".bloop" | ".metals" => CruftCategory::EditorCache,

        ".serverless" | ".vercel" | ".netlify" | ".terraform" | ".sass-cache" | ".cpcache"
        | "elm_stuff" | "nimcache" | ".dvc" | ".mlruns" | "outputs" => {
            CruftCategory::DeploymentCache
        }

        ".npm" | ".pnpm-store" | ".angular" | ".vite" | ".nx" | ".turbo" | ".parcel-cache"
        | ".rpt2_cache" | ".eslintcache" | ".esbuild" | ".cache" | ".rollup.cache" | ".swc"
        | ".stylelintcache" | "deno_cache" | "__pycache__" | ".mypy_cache" | ".ruff_cache"
        | ".pytype" | ".pyre" | ".gradle" | ".swiftpm" => CruftCategory::ToolCache,

        _ => CruftCategory::Unknown,
    }
}

fn delete_risk_for<'a>(
    target_name: &str,
    category: CruftCategory,
    is_custom: bool,
    risk: Option<&'a RiskAnalysis>,
) -> (DeleteRiskLevel, &'a str) {
    if let Some(analysis) = risk
        && analysis.is_sensitive
    {
        return (
            DeleteRiskLevel::High,
            analysis.reason.as_deref().unwrap_or("Sensitive path; review before deleting"),
        );
    }

    if risk.is_none() {
        return (DeleteRiskLevel::Medium, "Risk analysis disabled; inspect path before deleting");
    }

    if is_custom || category == CruftCategory::Unknown {
        return (DeleteRiskLevel::Medium, "Custom target; inspect contents before deleting");
    }

    if has_medium_recreate_cost(target_name, category) {
        return (
            DeleteRiskLevel::Medium,
            "Regenerable, but may require a longer rebuild or tool reinitialization",
        );
    }

    (DeleteRiskLevel::Low, "Regenerable cache or build output outside sensitive paths")
}

fn has_medium_recreate_cost(target_name: &str, category: CruftCategory) -> bool {
    matches!(category, CruftCategory::DeploymentCache)
        || matches!(
            target_name,
            "Library" | "DerivedData" | "DerivedDataCache" | "Binaries" | "Intermediate"
        )
}

fn rebuild_hint_for(target_name: &str, category: CruftCategory) -> Option<&'static str> {
    match category {
        CruftCategory::DependencyTree => Some("Reinstall dependencies before next build"),
        CruftCategory::VirtualEnvironment => {
            Some("Recreate with the project setup or package manager command")
        }
        CruftCategory::BuildOutput => Some("Build tool will recreate it on next build"),
        CruftCategory::TestCache => Some("Test runner will recreate it on next run"),
        CruftCategory::ToolCache => Some("Tool will recreate cache as needed"),
        CruftCategory::EditorCache => Some("Editor or language server will rebuild it"),
        CruftCategory::DeploymentCache => {
            if matches!(target_name, ".terraform") {
                Some("Usually recreated by terraform init; verify no state lives inside")
            } else {
                Some("Deployment tool may need init or login again")
            }
        }
        CruftCategory::Unknown => Some("Custom target; verify contents first"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_modules_maps_to_node_dependency_tree() {
        let risk = RiskAnalysis::safe();
        let meta = classify_target("node_modules", Some(&risk));
        assert_eq!(meta.ecosystems, vec!["node"]);
        assert_eq!(meta.category, CruftCategory::DependencyTree);
        assert_eq!(meta.delete_risk, DeleteRiskLevel::Low);
    }

    #[test]
    fn target_lists_all_matching_ecosystems() {
        let risk = RiskAnalysis::safe();
        let meta = classify_target("target", Some(&risk));
        assert_eq!(meta.ecosystems, vec!["java", "rust", "scala"]);
        assert_eq!(meta.category, CruftCategory::BuildOutput);
    }

    #[test]
    fn sensitive_path_risk_overrides_target_safety() {
        let risk = RiskAnalysis::sensitive("Contains user configuration data (~/.config)");
        let meta = classify_target("node_modules", Some(&risk));
        assert_eq!(meta.delete_risk, DeleteRiskLevel::High);
        assert_eq!(meta.delete_risk_reason, "Contains user configuration data (~/.config)");
    }

    #[test]
    fn missing_risk_analysis_is_medium_risk() {
        let meta = classify_target("node_modules", None);
        assert_eq!(meta.category, CruftCategory::DependencyTree);
        assert_eq!(meta.delete_risk, DeleteRiskLevel::Medium);
        assert!(meta.delete_risk_reason.contains("Risk analysis disabled"));
    }

    #[test]
    fn custom_target_is_medium_risk_and_unknown() {
        let risk = RiskAnalysis::safe();
        let meta = classify_target("my-cache", Some(&risk));
        assert!(meta.ecosystems.is_empty());
        assert_eq!(meta.category, CruftCategory::Unknown);
        assert_eq!(meta.delete_risk, DeleteRiskLevel::Medium);
    }

    #[test]
    fn all_profile_targets_have_non_unknown_categories() {
        let risk = RiskAnalysis::safe();
        for profile in base_profiles().values() {
            for target in profile.targets {
                let meta = classify_target(target, Some(&risk));
                assert_ne!(meta.category, CruftCategory::Unknown, "missing category for {target}");
            }
        }
    }
}

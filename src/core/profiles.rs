//! Hardcoded scan profiles — port of npkill's `BASE_PROFILES`.
//!
//! Source: `src/core/constants/profiles.constants.ts`. Each profile lists the
//! exact directory basenames the scanner should match. Target lists are kept
//! verbatim so npkill users get the same coverage.
//!
//! `resolve_targets(&["node"])` returns the union of one or more profiles'
//! targets (deduped, sorted). The special name `"all"` expands to the union
//! of every base profile.
//!
//! Decision C4: v0.1 ships hardcoded only; user-defined profiles via TOML are
//! deferred to v0.2.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Name of the profile applied when the user passes no `--profile` flag.
pub const DEFAULT_PROFILE: &str = "node";

/// One scan profile: a description and an exact-basename target list.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub description: &'static str,
    pub targets: &'static [&'static str],
}

/// Returns the (lazy-initialized) base profile registry.
///
/// `"all"` is synthesized on demand from the union of every entry's targets
/// (see [`resolve_targets`]).
pub fn base_profiles() -> &'static HashMap<&'static str, Profile> {
    static M: OnceLock<HashMap<&'static str, Profile>> = OnceLock::new();
    M.get_or_init(build_profiles)
}

fn build_profiles() -> HashMap<&'static str, Profile> {
    let mut m: HashMap<&'static str, Profile> = HashMap::new();

    m.insert("node", Profile {
        description: "All the usual suspects related with the node/web/javascript dev toolchain: \
                      node_modules, caches, build artifacts, and assorted JavaScript junk. \
                      Safe to clean and your disk will thank you.",
        targets: &[
            "node_modules",
            ".npm",
            ".pnpm-store",
            ".next",
            ".nuxt",
            ".angular",
            ".svelte-kit",
            ".vite",
            ".nx",
            ".turbo",
            ".parcel-cache",
            ".rpt2_cache",
            ".eslintcache",
            ".esbuild",
            ".cache",
            ".rollup.cache",
            "storybook-static",
            "coverage",
            ".nyc_output",
            ".jest",
            "gatsby_cache",
            ".docusaurus",
            ".swc",
            ".stylelintcache",
            "deno_cache",
        ],
    });

    m.insert(
        "python",
        Profile {
            description: "The usual Python leftovers — caches, virtual environments, and test \
                      artifacts. Safe to clear once you've closed your IDE and virtualenvs.",
            targets: &[
                "__pycache__",
                ".pytest_cache",
                ".mypy_cache",
                ".ruff_cache",
                ".tox",
                ".nox",
                ".pytype",
                ".pyre",
                "htmlcov",
                ".venv",
                "venv",
            ],
        },
    );

    m.insert(
        "data-science",
        Profile {
            description: "Jupyter checkpoints, virtualenvs, MLflow runs, and experiment outputs. \
                      Great for learning, terrible for disk space.",
            targets: &[
                ".ipynb_checkpoints",
                "__pycache__",
                ".venv",
                "venv",
                "outputs",
                ".dvc",
                ".mlruns",
            ],
        },
    );

    m.insert(
        "java",
        Profile {
            description: "Build outputs and Gradle junk.",
            targets: &["target", ".gradle", "out"],
        },
    );

    m.insert(
        "android",
        Profile {
            description: "Native build caches and intermediate files from Android Studio. Deleting \
                      won't hurt, but expect a rebuild marathon next time.",
            targets: &[".cxx", "externalNativeBuild"],
        },
    );

    m.insert(
        "swift",
        Profile {
            description: "Xcode's playground leftovers and Swift package builds. Heavy, harmless, \
                      and happy to go.",
            targets: &["DerivedData", ".swiftpm"],
        },
    );

    m.insert(
        "dotnet",
        Profile {
            description: "Compilation artifacts and Visual Studio cache folders. Disposable once \
                      you're done building or testing.",
            targets: &["obj", "TestResults", ".vs"],
        },
    );

    m.insert(
        "rust",
        Profile {
            description: "Cargo build targets. Huge, regenerable, and surprisingly clingy, your \
                      disk will appreciate the reset.",
            targets: &["target"],
        },
    );

    m.insert(
        "ruby",
        Profile { description: "Bundler caches and dependency leftovers.", targets: &[".bundle"] },
    );

    m.insert("elixir", Profile {
        description: "Mix build folders, dependencies, and coverage reports. Easy to regenerate, \
                      safe to purge.",
        targets: &["_build", "deps", "cover"],
    });

    m.insert(
        "haskell",
        Profile {
            description: "GHC and Stack build outputs. A collection of intermediate binaries you \
                      definitely don't need anymore.",
            targets: &["dist-newstyle", ".stack-work"],
        },
    );

    m.insert(
        "scala",
        Profile {
            description: "Bloop, Metals, and build outputs from Scala projects.",
            targets: &[".bloop", ".metals", "target"],
        },
    );

    m.insert(
        "cpp",
        Profile {
            description: "CMake build directories and temporary artifacts. Rebuilds take time, but \
                      space is priceless.",
            targets: &["CMakeFiles", "cmake-build-debug", "cmake-build-release"],
        },
    );

    m.insert(
        "unity",
        Profile {
            description: "Unity's cache and build artifacts. Expect longer load times next launch \
                      but it can save tons of space on unused projects.",
            targets: &["Library", "Temp", "Obj"],
        },
    );

    m.insert(
        "unreal",
        Profile {
            description: "Intermediate and binary build caches. Safe to clean. Unreal will \
                      (happily?) recompile.",
            targets: &["Intermediate", "DerivedDataCache", "Binaries"],
        },
    );

    m.insert(
        "godot",
        Profile {
            description: "Editor caches and import data. Godot can recreate these in a blink.",
            targets: &[".import", ".godot"],
        },
    );

    m.insert(
        "infra",
        Profile {
            description: "Leftovers from deployment tools like Serverless, Vercel, Netlify, and \
                      Terraform.",
            targets: &[
                ".serverless",
                ".vercel",
                ".netlify",
                ".terraform",
                ".sass-cache",
                ".cpcache",
                "elm_stuff",
                "nimcache",
                "deno_cache",
            ],
        },
    );

    m
}

/// Resolve one or more profile names into a deduped, sorted target list.
///
/// `"all"` expands to the union of every base profile's targets. Unknown
/// profile names are silently skipped (the CLI layer is expected to warn).
pub fn resolve_targets(names: &[&str]) -> Vec<String> {
    let profiles = base_profiles();
    let mut set: BTreeSet<String> = BTreeSet::new();

    for n in names {
        if *n == "all" {
            for p in profiles.values() {
                for t in p.targets {
                    set.insert((*t).to_string());
                }
            }
        } else if let Some(p) = profiles.get(n) {
            for t in p.targets {
                set.insert((*t).to_string());
            }
        }
    }
    set.into_iter().collect()
}

/// All known profile names (for CLI `--help` and validation).
pub fn profile_names() -> Vec<&'static str> {
    let mut names: Vec<_> = base_profiles().keys().copied().collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_exists() {
        assert!(base_profiles().contains_key(DEFAULT_PROFILE));
    }

    #[test]
    fn node_profile_includes_node_modules() {
        let p = base_profiles().get("node").unwrap();
        assert!(p.targets.contains(&"node_modules"));
        assert!(p.targets.contains(&".next"));
    }

    #[test]
    fn rust_profile_targets_target() {
        let p = base_profiles().get("rust").unwrap();
        assert_eq!(p.targets, &["target"]);
    }

    #[test]
    fn resolve_node_returns_node_modules() {
        let t = resolve_targets(&["node"]);
        assert!(t.contains(&"node_modules".to_string()));
        assert!(t.contains(&".npm".to_string()));
    }

    #[test]
    fn resolve_all_is_union_of_everything() {
        let t = resolve_targets(&["all"]);
        // node_modules from node profile + target from rust profile + __pycache__ from python
        assert!(t.contains(&"node_modules".to_string()));
        assert!(t.contains(&"target".to_string()));
        assert!(t.contains(&"__pycache__".to_string()));
    }

    #[test]
    fn resolve_dedupes_across_profiles() {
        // "deno_cache" appears in both node and infra; "__pycache__" in both python and data-science
        let t = resolve_targets(&["node", "infra"]);
        let count = t.iter().filter(|s| s.as_str() == "deno_cache").count();
        assert_eq!(count, 1, "expected dedup, got {count} copies");
    }

    #[test]
    fn resolve_unknown_profile_is_silently_skipped() {
        let t = resolve_targets(&["this-does-not-exist"]);
        assert!(t.is_empty());
    }

    #[test]
    fn profile_names_includes_known_entries() {
        let names = profile_names();
        for required in ["node", "python", "rust", "java", "swift"] {
            assert!(names.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn at_least_seventeen_profiles_present() {
        // Plan promised 17 base profiles (node, python, data-science, java, android,
        // swift, dotnet, rust, ruby, elixir, haskell, scala, cpp, unity, unreal,
        // godot, infra).
        assert!(
            base_profiles().len() >= 17,
            "expected ≥17 profiles, got {}",
            base_profiles().len()
        );
    }
}

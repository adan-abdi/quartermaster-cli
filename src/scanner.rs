use anyhow::{anyhow, Result};
use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name: String,
    pub path: PathBuf,
    pub source_type: SourceType,
    pub files: Vec<FileInfo>,
    pub git_info: Option<GitInfo>,
    pub selected_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedRepo {
    pub name: String,
    pub path: PathBuf,
    pub source_type: SourceType,
    pub git_info: Option<GitInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    GitHub {
        url: String,
        owner: String,
        repo: String,
    },
    Local {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootEntry {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub relative_path: String,
    pub size: u64,
    pub file_type: FileType,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileType {
    SourceCode { language: String },
    Config,
    Documentation,
    Test,
    Asset,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: String,
    pub commit: String,
    pub remote_url: Option<String>,
    pub contributors: Vec<GitContributor>,
    pub diff_summary: GitDiffSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitContributor {
    pub name: String,
    pub email: Option<String>,
    pub commits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitDiffSummary {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub renamed: usize,
    pub untracked: usize,
    pub shortstat: Option<String>,
}

pub struct RepoScanner {
    respect_gitignore: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum StackProfile {
    Node,
    Rust,
    Python,
    Java,
    Go,
    Ruby,
    Php,
    DotNet,
}

impl RepoScanner {
    pub fn new(respect_gitignore: bool) -> Self {
        Self { respect_gitignore }
    }

    pub fn prepare(&self, source: &str) -> Result<PreparedRepo> {
        let source_type = self.determine_source_type(source)?;

        match source_type {
            SourceType::GitHub { url, owner, repo } => {
                let repo_path = self.clone_github_repo(&url, &owner, &repo)?;
                Ok(PreparedRepo {
                    name: repo.clone(),
                    path: repo_path.clone(),
                    source_type: SourceType::GitHub { url, owner, repo },
                    git_info: self.get_git_info(&repo_path),
                })
            }
            SourceType::Local { path } => {
                if !path.exists() {
                    return Err(anyhow!("Local path does not exist: {}", path.display()));
                }

                let repo_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("workspace")
                    .to_string();

                Ok(PreparedRepo {
                    name: repo_name,
                    path: path.clone(),
                    source_type: SourceType::Local { path: path.clone() },
                    git_info: self.get_git_info(&path),
                })
            }
        }
    }

    pub fn list_root_entries(
        &self,
        repo_path: &Path,
        workspace_dir_name: &str,
    ) -> Result<Vec<RootEntry>> {
        let stack_profiles = self.detect_stack_profiles(repo_path)?;
        let stack_ignores = self.build_stack_ignore_names(&stack_profiles);
        let mut builder = WalkBuilder::new(repo_path);
        builder.max_depth(Some(1));
        builder.hidden(false);

        if !self.respect_gitignore {
            builder
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false);
        }

        let mut entries = Vec::new();

        for result in builder.build() {
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let path = entry.path();
            if path == repo_path {
                continue;
            }

            let relative_path = match path.strip_prefix(repo_path) {
                Ok(path) => normalize_path(path),
                Err(_) => continue,
            };

            if self.should_skip_relative_path(&relative_path, workspace_dir_name, &stack_ignores) {
                continue;
            }

            let file_type = match entry.file_type() {
                Some(file_type) => file_type,
                None => continue,
            };

            entries.push(RootEntry {
                name: relative_path.clone(),
                relative_path,
                is_dir: file_type.is_dir(),
            });
        }

        entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(entries)
    }

    pub fn scan_prepared(
        &self,
        prepared: PreparedRepo,
        selected_roots: &[String],
        workspace_dir_name: &str,
    ) -> Result<RepoInfo> {
        let files = self.scan_files(&prepared.path, selected_roots, workspace_dir_name)?;
        let mut roots = selected_roots.to_vec();
        roots.sort();

        Ok(RepoInfo {
            name: prepared.name,
            path: prepared.path,
            source_type: prepared.source_type,
            files,
            git_info: prepared.git_info,
            selected_roots: roots,
        })
    }

    fn determine_source_type(&self, source: &str) -> Result<SourceType> {
        let github_regex =
            Regex::new(r"github\.com[:/](?P<owner>[^/]+)/(?P<repo>[^/]+?)(?:\.git)?$")?;

        if let Some(captures) = github_regex.captures(source) {
            let owner = captures["owner"].to_string();
            let repo = captures["repo"].to_string();
            let url = if source.starts_with("http") {
                source.to_string()
            } else {
                format!("https://github.com/{owner}/{repo}.git")
            };

            return Ok(SourceType::GitHub { url, owner, repo });
        }

        let path = Path::new(source);
        if path.exists() {
            return Ok(SourceType::Local {
                path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
            });
        }

        Err(anyhow!(
            "Invalid source: must be a local path or GitHub URL"
        ))
    }

    fn clone_github_repo(&self, url: &str, owner: &str, repo: &str) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let clone_path = temp_dir
            .join("quartermaster")
            .join(format!("{owner}_{repo}"));

        if clone_path.exists() {
            fs::remove_dir_all(&clone_path)?;
        }

        if let Some(parent) = clone_path.parent() {
            fs::create_dir_all(parent)?;
        }

        println!("🚀 Cloning repository from {url}...");

        let output = Command::new("git")
            .args(["clone", url, &clone_path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to clone repository: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(clone_path)
    }

    fn scan_files(
        &self,
        repo_path: &Path,
        selected_roots: &[String],
        workspace_dir_name: &str,
    ) -> Result<Vec<FileInfo>> {
        let stack_profiles = self.detect_stack_profiles(repo_path)?;
        let stack_ignores = self.build_stack_ignore_names(&stack_profiles);
        let selected_root_set: HashSet<String> = selected_roots.iter().cloned().collect();
        let limit_to_selected_roots = !selected_root_set.is_empty();

        let mut builder = WalkBuilder::new(repo_path);
        builder.hidden(false);

        if !self.respect_gitignore {
            builder
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false);
        }

        let mut files = Vec::new();

        for result in builder.build() {
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let entry_path = entry.path();
            if entry_path == repo_path {
                continue;
            }

            let relative_path = match entry_path.strip_prefix(repo_path) {
                Ok(path) => normalize_path(path),
                Err(error) => return Err(anyhow!("Failed to compute relative path: {error}")),
            };

            if self.should_skip_relative_path(&relative_path, workspace_dir_name, &stack_ignores) {
                continue;
            }

            if limit_to_selected_roots {
                let top_level = relative_path
                    .split('/')
                    .next()
                    .unwrap_or(relative_path.as_str())
                    .to_string();

                if !selected_root_set.contains(&top_level) {
                    continue;
                }
            }

            let file_type = match entry.file_type() {
                Some(file_type) => file_type,
                None => continue,
            };

            if file_type.is_dir() {
                continue;
            }

            let metadata = fs::metadata(entry_path)?;
            let size = metadata.len();
            let resolved_file_type = self.determine_file_type(entry_path);

            let content = if self.should_read_content(&resolved_file_type, size) {
                fs::read_to_string(entry_path).ok()
            } else {
                None
            };

            files.push(FileInfo {
                path: entry_path.to_path_buf(),
                relative_path,
                size,
                file_type: resolved_file_type,
                content,
            });
        }

        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(files)
    }

    fn should_skip_relative_path(
        &self,
        relative_path: &str,
        workspace_dir_name: &str,
        stack_ignores: &HashSet<String>,
    ) -> bool {
        let generated_dir = workspace_dir_name.trim_matches('/');
        let ignored_names = [
            ".git", "build", "dist", "coverage", ".cache", ".turbo", ".idea", ".vscode",
        ];

        relative_path.split('/').any(|component| {
            component == generated_dir
                || ignored_names.contains(&component)
                || stack_ignores.contains(component)
        })
    }

    fn detect_stack_profiles(&self, repo_path: &Path) -> Result<HashSet<StackProfile>> {
        let mut profiles = HashSet::new();
        let mut builder = WalkBuilder::new(repo_path);
        builder.max_depth(Some(3));
        builder.hidden(false);
        builder
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false);

        for result in builder.build() {
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let path = entry.path();
            if path == repo_path || entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }

            let relative_path = match path.strip_prefix(repo_path) {
                Ok(path) => normalize_path(path).to_lowercase(),
                Err(_) => continue,
            };

            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_lowercase();

            if matches!(
                file_name.as_str(),
                "package.json" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "bun.lockb"
            ) || matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")
            ) {
                profiles.insert(StackProfile::Node);
            }

            if file_name == "cargo.toml"
                || matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("rs")
                )
            {
                profiles.insert(StackProfile::Rust);
            }

            if matches!(
                file_name.as_str(),
                "pyproject.toml" | "requirements.txt" | "poetry.lock" | "pipfile"
            ) || matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("py")
            ) {
                profiles.insert(StackProfile::Python);
            }

            if file_name == "go.mod"
                || matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("go")
                )
            {
                profiles.insert(StackProfile::Go);
            }

            if matches!(
                file_name.as_str(),
                "pom.xml" | "build.gradle" | "build.gradle.kts" | "settings.gradle"
            ) || matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("java" | "kt")
            ) {
                profiles.insert(StackProfile::Java);
            }

            if matches!(file_name.as_str(), "gemfile" | "gemfile.lock")
                || matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("rb")
                )
            {
                profiles.insert(StackProfile::Ruby);
            }

            if file_name == "composer.json"
                || matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("php")
                )
            {
                profiles.insert(StackProfile::Php);
            }

            if file_name.ends_with(".csproj")
                || file_name.ends_with(".sln")
                || matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("cs")
                )
            {
                profiles.insert(StackProfile::DotNet);
            }

            if relative_path.contains("/.venv/") || relative_path.contains("/venv/") {
                profiles.insert(StackProfile::Python);
            }
        }

        Ok(profiles)
    }

    fn build_stack_ignore_names(&self, stack_profiles: &HashSet<StackProfile>) -> HashSet<String> {
        let mut ignored = HashSet::new();

        for profile in stack_profiles {
            match profile {
                StackProfile::Node => {
                    ignored.extend(
                        [
                            "node_modules",
                            ".next",
                            ".nuxt",
                            ".svelte-kit",
                            ".parcel-cache",
                            ".pnpm-store",
                            ".yarn",
                            ".vercel",
                            ".output",
                            ".angular",
                            ".astro",
                            ".expo",
                        ]
                        .into_iter()
                        .map(String::from),
                    );
                }
                StackProfile::Rust => {
                    ignored.extend(["target"].into_iter().map(String::from));
                }
                StackProfile::Python => {
                    ignored.extend(
                        [
                            "__pycache__",
                            ".mypy_cache",
                            ".pytest_cache",
                            ".ruff_cache",
                            ".tox",
                            ".nox",
                            ".venv",
                            "venv",
                            "env",
                            ".eggs",
                            ".ipynb_checkpoints",
                        ]
                        .into_iter()
                        .map(String::from),
                    );
                }
                StackProfile::Java => {
                    ignored.extend([".gradle", "out"].into_iter().map(String::from));
                }
                StackProfile::Go => {
                    ignored.extend(["vendor"].into_iter().map(String::from));
                }
                StackProfile::Ruby => {
                    ignored.extend([".bundle", "vendor"].into_iter().map(String::from));
                }
                StackProfile::Php => {
                    ignored.extend(["vendor"].into_iter().map(String::from));
                }
                StackProfile::DotNet => {
                    ignored.extend(["bin", "obj", ".vs"].into_iter().map(String::from));
                }
            }
        }

        ignored
    }

    fn should_read_content(&self, file_type: &FileType, size: u64) -> bool {
        if size > 1_500_000 {
            return false;
        }

        matches!(
            file_type,
            FileType::SourceCode { .. }
                | FileType::Config
                | FileType::Documentation
                | FileType::Test
        )
    }

    fn determine_file_type(&self, path: &Path) -> FileType {
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            match extension.to_lowercase().as_str() {
                "rs" => FileType::SourceCode {
                    language: "Rust".to_string(),
                },
                "js" | "jsx" => FileType::SourceCode {
                    language: "JavaScript".to_string(),
                },
                "ts" | "tsx" => FileType::SourceCode {
                    language: "TypeScript".to_string(),
                },
                "py" => FileType::SourceCode {
                    language: "Python".to_string(),
                },
                "go" => FileType::SourceCode {
                    language: "Go".to_string(),
                },
                "java" => FileType::SourceCode {
                    language: "Java".to_string(),
                },
                "cpp" | "cxx" | "cc" => FileType::SourceCode {
                    language: "C++".to_string(),
                },
                "c" => FileType::SourceCode {
                    language: "C".to_string(),
                },
                "cs" => FileType::SourceCode {
                    language: "C#".to_string(),
                },
                "php" => FileType::SourceCode {
                    language: "PHP".to_string(),
                },
                "rb" => FileType::SourceCode {
                    language: "Ruby".to_string(),
                },
                "swift" => FileType::SourceCode {
                    language: "Swift".to_string(),
                },
                "kt" => FileType::SourceCode {
                    language: "Kotlin".to_string(),
                },
                "scala" => FileType::SourceCode {
                    language: "Scala".to_string(),
                },
                "html" | "htm" => FileType::SourceCode {
                    language: "HTML".to_string(),
                },
                "css" | "scss" | "sass" | "less" => FileType::SourceCode {
                    language: "CSS".to_string(),
                },
                "sh" | "bash" | "zsh" => FileType::SourceCode {
                    language: "Shell".to_string(),
                },
                "json" | "yaml" | "yml" | "toml" | "xml" => FileType::Config,
                "md" | "mdx" | "rst" => FileType::Documentation,
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "bmp" | "webp" | "pdf" => {
                    FileType::Asset
                }
                _ => {
                    if path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .map(|name| name.contains(".test.") || name.contains(".spec."))
                        .unwrap_or(false)
                    {
                        FileType::Test
                    } else {
                        FileType::Other
                    }
                }
            }
        } else {
            let lowercase_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_lowercase();

            match lowercase_name.as_str() {
                "dockerfile" | "docker-compose" | "makefile" | "justfile" | "procfile" => {
                    FileType::Config
                }
                "readme" | "license" | "changelog" | "contributing" => FileType::Documentation,
                _ => FileType::Other,
            }
        }
    }

    fn get_git_info(&self, repo_path: &Path) -> Option<GitInfo> {
        let get_git_output = |args: &[&str]| -> Option<String> {
            Command::new("git")
                .args(args)
                .current_dir(repo_path)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|output| output.trim().to_string())
        };

        let branch = get_git_output(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        let commit = get_git_output(&["rev-parse", "HEAD"])?;
        let remote_url = get_git_output(&["config", "--get", "remote.origin.url"]);
        let contributors = self.get_git_contributors(repo_path);
        let diff_summary = self.get_git_diff_summary(repo_path);

        Some(GitInfo {
            branch,
            commit,
            remote_url,
            contributors,
            diff_summary,
        })
    }

    fn get_git_contributors(&self, repo_path: &Path) -> Vec<GitContributor> {
        let output = Command::new("git")
            .args(["shortlog", "-sne", "HEAD"])
            .current_dir(repo_path)
            .output();

        let stdout = match output {
            Ok(output) if output.status.success() => {
                String::from_utf8(output.stdout).unwrap_or_default()
            }
            _ => return Vec::new(),
        };

        stdout
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }

                let mut pieces = trimmed.splitn(2, '\t');
                let count = pieces.next()?.trim().parse::<usize>().ok()?;
                let identity = pieces.next()?.trim();

                if let Some((name, rest)) = identity.rsplit_once('<') {
                    Some(GitContributor {
                        name: name.trim().to_string(),
                        email: Some(rest.trim_end_matches('>').trim().to_string()),
                        commits: count,
                    })
                } else {
                    Some(GitContributor {
                        name: identity.to_string(),
                        email: None,
                        commits: count,
                    })
                }
            })
            .collect()
    }

    fn get_git_diff_summary(&self, repo_path: &Path) -> GitDiffSummary {
        let status_output = Command::new("git")
            .args(["status", "--short"])
            .current_dir(repo_path)
            .output();

        let mut summary = GitDiffSummary::default();

        if let Ok(output) = status_output {
            if output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap_or_default();

                for line in stdout.lines() {
                    let status = line.chars().take(2).collect::<String>();
                    match status.as_str() {
                        "??" => summary.untracked += 1,
                        _ => {
                            let chars: Vec<char> = status.chars().collect();
                            if chars.iter().any(|value| *value == 'A') {
                                summary.added += 1;
                            }
                            if chars.iter().any(|value| *value == 'M') {
                                summary.modified += 1;
                            }
                            if chars.iter().any(|value| *value == 'D') {
                                summary.deleted += 1;
                            }
                            if chars.iter().any(|value| *value == 'R') {
                                summary.renamed += 1;
                            }
                        }
                    }
                }
            }
        }

        summary.shortstat = Command::new("git")
            .args(["diff", "--shortstat", "HEAD"])
            .current_dir(repo_path)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|output| output.trim().to_string())
            .filter(|output| !output.is_empty());

        summary
    }
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|value| value.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

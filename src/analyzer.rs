use crate::scanner::{FileInfo, FileType, GitContributor, GitDiffSummary, RepoInfo};
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub total_files: usize,
    pub lines_of_code: usize,
    pub tech_stack: Vec<String>,
    pub dependencies: DependencyMap,
    pub file_tree: FileTreeNode,
    pub statistics: LanguageStats,
    pub overview: OverviewStats,
    pub graph: KnowledgeGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyMap {
    pub imports: HashMap<String, Vec<DependencyReference>>,
    pub exports: HashMap<String, Vec<String>>,
    pub internal_references: HashMap<String, Vec<String>>,
    pub dependents: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyReference {
    pub specifier: String,
    pub kind: DependencyKind,
    pub resolved_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyKind {
    Internal,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub node_type: NodeType,
    pub children: Vec<FileTreeNode>,
    pub file_summary: Option<FileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSummary {
    pub relative_path: String,
    pub file_type: String,
    pub language: Option<String>,
    pub size: u64,
    pub lines_of_code: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Directory,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageStats {
    pub languages: BTreeMap<String, LanguageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub file_count: usize,
    pub lines_of_code: usize,
    pub file_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStats {
    pub total_files: usize,
    pub total_directories: usize,
    pub lines_of_code: usize,
    pub tech_stack: Vec<String>,
    pub selected_roots: Vec<String>,
    pub largest_files: Vec<FileMetric>,
    pub files_by_type: BTreeMap<String, usize>,
    pub contributor_count: usize,
    pub contributors: Vec<GitContributor>,
    pub git_diff: Option<GitDiffSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetric {
    pub path: String,
    pub size: u64,
    pub lines_of_code: usize,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub path: String,
    pub node_type: GraphNodeType,
    pub weight: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphNodeType {
    File,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

pub struct RepoAnalyzer;

impl RepoAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, repo_info: &RepoInfo) -> Result<AnalysisResult> {
        let total_files = repo_info.files.len();
        let lines_of_code = self.count_lines_of_code(&repo_info.files);
        let tech_stack = self.detect_tech_stack(&repo_info.files);
        let dependencies = self.extract_dependencies(&repo_info.files);
        let file_tree = self.build_file_tree(&repo_info.files);
        let statistics = self.calculate_language_stats(&repo_info.files);
        let overview = self.build_overview(repo_info, lines_of_code, &tech_stack, &file_tree);
        let graph = self.build_graph(&repo_info.files, &dependencies);

        Ok(AnalysisResult {
            total_files,
            lines_of_code,
            tech_stack,
            dependencies,
            file_tree,
            statistics,
            overview,
            graph,
        })
    }

    fn count_lines_of_code(&self, files: &[FileInfo]) -> usize {
        files
            .iter()
            .filter(|file| matches!(file.file_type, FileType::SourceCode { .. } | FileType::Test))
            .map(|file| {
                file.content
                    .as_ref()
                    .map(|content| content.lines().count())
                    .unwrap_or(0)
            })
            .sum()
    }

    fn detect_tech_stack(&self, files: &[FileInfo]) -> Vec<String> {
        let mut languages = HashSet::new();
        let mut frameworks = HashSet::new();
        let mut tools = HashSet::new();

        for file in files {
            if let FileType::SourceCode { language } = &file.file_type {
                languages.insert(language.clone());
            }

            let path = file.relative_path.to_lowercase();

            if path.ends_with("package.json") {
                frameworks.insert("Node.js".to_string());
                tools.insert("npm".to_string());
            }
            if path.ends_with("yarn.lock") {
                tools.insert("yarn".to_string());
            }
            if path.ends_with("pnpm-lock.yaml") {
                tools.insert("pnpm".to_string());
            }
            if path.ends_with("cargo.toml") {
                frameworks.insert("Rust".to_string());
                tools.insert("Cargo".to_string());
            }
            if path.ends_with("pyproject.toml") || path.ends_with("requirements.txt") {
                frameworks.insert("Python".to_string());
                tools.insert("pip".to_string());
            }
            if path.ends_with("go.mod") {
                frameworks.insert("Go".to_string());
            }
            if path.ends_with("pom.xml") {
                frameworks.insert("Java".to_string());
                tools.insert("Maven".to_string());
            }
            if path.ends_with("build.gradle") {
                frameworks.insert("Java".to_string());
                tools.insert("Gradle".to_string());
            }
            if path.contains("react") || path.ends_with(".tsx") || path.ends_with(".jsx") {
                frameworks.insert("React".to_string());
            }
            if path.contains("next.config") {
                frameworks.insert("Next.js".to_string());
            }
            if path.contains("vite.config") {
                tools.insert("Vite".to_string());
            }
            if path.contains("tailwind.config") {
                tools.insert("Tailwind CSS".to_string());
            }
            if path.contains("eslint") {
                tools.insert("ESLint".to_string());
            }
            if path.contains("vitest") || path.contains("jest") {
                tools.insert("Testing".to_string());
            }
            if path.contains("dockerfile") || path.contains("docker-compose") {
                tools.insert("Docker".to_string());
            }
        }

        let mut tech_stack: Vec<String> = languages
            .into_iter()
            .chain(frameworks.into_iter())
            .chain(tools.into_iter())
            .collect();

        tech_stack.sort();
        tech_stack.dedup();
        tech_stack
    }

    fn extract_dependencies(&self, files: &[FileInfo]) -> DependencyMap {
        let known_paths = files
            .iter()
            .map(|file| normalize_path(&file.relative_path))
            .collect::<HashSet<_>>();

        let mut imports = HashMap::new();
        let mut exports = HashMap::new();
        let mut internal_references = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for file in files {
            let Some(content) = &file.content else {
                continue;
            };

            let detected = match &file.file_type {
                FileType::SourceCode { language } => self.extract_language_dependencies(
                    language,
                    &file.relative_path,
                    content,
                    &known_paths,
                ),
                FileType::Test => self.extract_language_dependencies(
                    self.infer_language_from_path(&file.relative_path)
                        .as_deref()
                        .unwrap_or(""),
                    &file.relative_path,
                    content,
                    &known_paths,
                ),
                _ => continue,
            };

            if !detected.imports.is_empty() {
                let references = detected
                    .imports
                    .iter()
                    .filter_map(|dependency| dependency.resolved_path.clone())
                    .collect::<Vec<_>>();

                for target in &references {
                    dependents
                        .entry(target.clone())
                        .or_default()
                        .push(file.relative_path.clone());
                }

                internal_references.insert(file.relative_path.clone(), references);
                imports.insert(file.relative_path.clone(), detected.imports);
            }

            if !detected.exports.is_empty() {
                exports.insert(file.relative_path.clone(), detected.exports);
            }
        }

        for values in dependents.values_mut() {
            values.sort();
            values.dedup();
        }

        DependencyMap {
            imports,
            exports,
            internal_references,
            dependents,
        }
    }

    fn extract_language_dependencies(
        &self,
        language: &str,
        file_path: &str,
        content: &str,
        known_paths: &HashSet<String>,
    ) -> ExtractedDependencies {
        match language {
            "JavaScript" | "TypeScript" => {
                self.extract_javascript_dependencies(file_path, content, known_paths)
            }
            "Python" => self.extract_python_dependencies(file_path, content, known_paths),
            "Rust" => self.extract_rust_dependencies(file_path, content, known_paths),
            "Go" => self.extract_go_dependencies(file_path, content, known_paths),
            "Shell" => self.extract_shell_dependencies(file_path, content),
            _ => ExtractedDependencies::default(),
        }
    }

    fn extract_javascript_dependencies(
        &self,
        file_path: &str,
        content: &str,
        known_paths: &HashSet<String>,
    ) -> ExtractedDependencies {
        let import_regex = Regex::new(
            r#"(?m)(?:import\s+(?:.+?\s+from\s+)?|export\s+.+?\s+from\s+|require\s*\(|import\s*\()\s*['"]([^'"]+)['"]"#,
        )
        .unwrap();
        let export_regex = Regex::new(
            r#"(?m)export\s+(?:default\s+)?(?:async\s+)?(?:function|class|const|let|var|type|interface)\s+([A-Za-z0-9_]+)"#,
        )
        .unwrap();

        let mut imports = Vec::new();
        for capture in import_regex.captures_iter(content) {
            let specifier = capture
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            imports.push(self.build_reference(file_path, specifier, known_paths));
        }

        let exports = export_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
            .collect::<Vec<_>>();

        ExtractedDependencies { imports, exports }
    }

    fn extract_python_dependencies(
        &self,
        file_path: &str,
        content: &str,
        known_paths: &HashSet<String>,
    ) -> ExtractedDependencies {
        let import_regex =
            Regex::new(r#"(?m)^(?:from\s+([A-Za-z0-9_\.]+)\s+import|import\s+([A-Za-z0-9_\.]+))"#)
                .unwrap();
        let export_regex = Regex::new(r#"(?m)^(?:def|class)\s+([A-Za-z0-9_]+)"#).unwrap();

        let imports = import_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1).or_else(|| capture.get(2)))
            .map(|value| self.build_reference(file_path, value.as_str(), known_paths))
            .collect::<Vec<_>>();

        let exports = export_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
            .collect::<Vec<_>>();

        ExtractedDependencies { imports, exports }
    }

    fn extract_rust_dependencies(
        &self,
        file_path: &str,
        content: &str,
        known_paths: &HashSet<String>,
    ) -> ExtractedDependencies {
        let use_regex = Regex::new(r#"(?m)^use\s+([^;]+);"#).unwrap();
        let pub_regex = Regex::new(
            r#"(?m)^pub\s+(?:async\s+)?(?:fn|struct|enum|mod|trait|const)\s+([A-Za-z0-9_]+)"#,
        )
        .unwrap();

        let imports = use_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1))
            .map(|value| self.build_reference(file_path, value.as_str(), known_paths))
            .collect::<Vec<_>>();

        let exports = pub_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
            .collect::<Vec<_>>();

        ExtractedDependencies { imports, exports }
    }

    fn extract_go_dependencies(
        &self,
        file_path: &str,
        content: &str,
        known_paths: &HashSet<String>,
    ) -> ExtractedDependencies {
        let import_regex = Regex::new(r#"(?m)^\s*"([^"]+)""#).unwrap();
        let export_regex = Regex::new(r#"(?m)^(?:func|type|var|const)\s+([A-Za-z0-9_]+)"#).unwrap();

        let imports = import_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1))
            .map(|value| self.build_reference(file_path, value.as_str(), known_paths))
            .collect::<Vec<_>>();

        let exports = export_regex
            .captures_iter(content)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
            .filter(|name| {
                name.chars()
                    .next()
                    .map(|value| value.is_uppercase())
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();

        ExtractedDependencies { imports, exports }
    }

    fn extract_shell_dependencies(
        &self,
        _file_path: &str,
        _content: &str,
    ) -> ExtractedDependencies {
        ExtractedDependencies::default()
    }

    fn build_reference(
        &self,
        file_path: &str,
        specifier: &str,
        known_paths: &HashSet<String>,
    ) -> DependencyReference {
        let resolved_path = self.resolve_dependency(file_path, specifier, known_paths);
        let kind = if resolved_path.is_some() || self.is_internal_specifier(specifier) {
            DependencyKind::Internal
        } else {
            DependencyKind::External
        };

        DependencyReference {
            specifier: specifier.to_string(),
            kind,
            resolved_path,
        }
    }

    fn is_internal_specifier(&self, specifier: &str) -> bool {
        specifier.starts_with('.')
            || specifier.starts_with('/')
            || specifier.starts_with("@/")
            || specifier.starts_with("@dashboard/")
            || specifier.starts_with("crate::")
            || specifier.starts_with("self::")
            || specifier.starts_with("super::")
    }

    fn resolve_dependency(
        &self,
        file_path: &str,
        specifier: &str,
        known_paths: &HashSet<String>,
    ) -> Option<String> {
        if specifier.starts_with("@/") {
            return self.resolve_known_candidate(specifier.replacen("@/", "src/", 1), known_paths);
        }

        if specifier.starts_with("@dashboard/") {
            return self.resolve_known_candidate(
                specifier.replacen("@dashboard/", "dashboard/", 1),
                known_paths,
            );
        }

        if specifier.starts_with("./") || specifier.starts_with("../") {
            let parent = Path::new(file_path)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            let candidate = normalize_path_from_pathbuf(parent.join(specifier));
            return self.resolve_known_candidate(candidate, known_paths);
        }

        if specifier.starts_with('/') {
            return self.resolve_known_candidate(
                specifier.trim_start_matches('/').to_string(),
                known_paths,
            );
        }

        if specifier.starts_with("crate::")
            || specifier.starts_with("self::")
            || specifier.starts_with("super::")
        {
            let candidate = specifier
                .replace("crate::", "src/")
                .replace("self::", "")
                .replace("super::", "")
                .replace("::", "/");
            return self.resolve_known_candidate(candidate, known_paths);
        }

        if specifier.contains('.') {
            return self.resolve_known_candidate(specifier.replace('.', "/"), known_paths);
        }

        None
    }

    fn resolve_known_candidate(
        &self,
        candidate: String,
        known_paths: &HashSet<String>,
    ) -> Option<String> {
        let normalized = normalize_path(&candidate);

        let mut possibilities = vec![normalized.clone()];
        for extension in [
            ".ts", ".tsx", ".js", ".jsx", ".rs", ".py", ".go", ".md", ".mdx",
        ] {
            possibilities.push(format!("{normalized}{extension}"));
        }

        for extension in [
            ".ts", ".tsx", ".js", ".jsx", ".rs", ".py", ".go", ".md", ".mdx",
        ] {
            possibilities.push(format!("{normalized}/index{extension}"));
            possibilities.push(format!("{normalized}/mod{extension}"));
            possibilities.push(format!("{normalized}/__init__{extension}"));
        }

        possibilities
            .into_iter()
            .find(|path| known_paths.contains(path))
    }

    fn infer_language_from_path(&self, path: &str) -> Option<String> {
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|extension| match extension {
                "ts" | "tsx" => "TypeScript",
                "js" | "jsx" => "JavaScript",
                "py" => "Python",
                "rs" => "Rust",
                "go" => "Go",
                "sh" | "bash" | "zsh" => "Shell",
                _ => "",
            })
            .filter(|language| !language.is_empty())
            .map(|language| language.to_string())
    }

    fn build_file_tree(&self, files: &[FileInfo]) -> FileTreeNode {
        let mut root = FileTreeNode {
            name: ".".to_string(),
            path: ".".to_string(),
            node_type: NodeType::Directory,
            children: Vec::new(),
            file_summary: None,
        };

        for file in files {
            self.add_file_to_tree(&mut root, file);
        }

        sort_tree(&mut root);
        root
    }

    fn add_file_to_tree(&self, root: &mut FileTreeNode, file: &FileInfo) {
        let parts = file.relative_path.split('/').collect::<Vec<_>>();
        let mut current = root;

        for (index, part) in parts.iter().enumerate() {
            let is_file = index == parts.len() - 1;

            if is_file {
                current.children.push(FileTreeNode {
                    name: (*part).to_string(),
                    path: file.relative_path.clone(),
                    node_type: NodeType::File,
                    children: Vec::new(),
                    file_summary: Some(FileSummary {
                        relative_path: file.relative_path.clone(),
                        file_type: file_kind_label(&file.file_type),
                        language: file_language(&file.file_type),
                        size: file.size,
                        lines_of_code: file.content.as_ref().map(|content| content.lines().count()),
                    }),
                });
                break;
            }

            let next_path = parts[..=index].join("/");
            let existing_index = current.children.iter().position(|child| {
                child.name == *part && matches!(child.node_type, NodeType::Directory)
            });

            if let Some(existing_index) = existing_index {
                current = &mut current.children[existing_index];
            } else {
                current.children.push(FileTreeNode {
                    name: (*part).to_string(),
                    path: next_path,
                    node_type: NodeType::Directory,
                    children: Vec::new(),
                    file_summary: None,
                });
                current = current.children.last_mut().unwrap();
            }
        }
    }

    fn calculate_language_stats(&self, files: &[FileInfo]) -> LanguageStats {
        let mut languages = BTreeMap::new();

        for file in files {
            if let FileType::SourceCode { language } = &file.file_type {
                let entry = languages.entry(language.clone()).or_insert(LanguageInfo {
                    file_count: 0,
                    lines_of_code: 0,
                    file_types: Vec::new(),
                });

                entry.file_count += 1;
                entry.lines_of_code += file
                    .content
                    .as_ref()
                    .map(|content| content.lines().count())
                    .unwrap_or(0);

                let extension = file
                    .path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                if !entry.file_types.contains(&extension) {
                    entry.file_types.push(extension);
                    entry.file_types.sort();
                }
            }
        }

        LanguageStats { languages }
    }

    fn build_overview(
        &self,
        repo_info: &RepoInfo,
        lines_of_code: usize,
        tech_stack: &[String],
        file_tree: &FileTreeNode,
    ) -> OverviewStats {
        let mut files_by_type = BTreeMap::new();

        for file in &repo_info.files {
            let label = file_kind_label(&file.file_type);
            *files_by_type.entry(label).or_insert(0) += 1;
        }

        let mut largest_files = repo_info
            .files
            .iter()
            .map(|file| FileMetric {
                path: file.relative_path.clone(),
                size: file.size,
                lines_of_code: file
                    .content
                    .as_ref()
                    .map(|content| content.lines().count())
                    .unwrap_or(0),
                language: file_language(&file.file_type),
            })
            .collect::<Vec<_>>();

        largest_files
            .sort_by(|left, right| right.size.cmp(&left.size).then(left.path.cmp(&right.path)));
        largest_files.truncate(8);

        let (contributor_count, contributors, git_diff) = repo_info
            .git_info
            .as_ref()
            .map(|git_info| {
                (
                    git_info.contributors.len(),
                    git_info.contributors.clone(),
                    Some(git_info.diff_summary.clone()),
                )
            })
            .unwrap_or((0, Vec::new(), None));

        OverviewStats {
            total_files: repo_info.files.len(),
            total_directories: count_directories(file_tree),
            lines_of_code,
            tech_stack: tech_stack.to_vec(),
            selected_roots: repo_info.selected_roots.clone(),
            largest_files,
            files_by_type,
            contributor_count,
            contributors,
            git_diff,
        }
    }

    fn build_graph(&self, files: &[FileInfo], dependencies: &DependencyMap) -> KnowledgeGraph {
        let mut nodes = files
            .iter()
            .map(|file| GraphNode {
                id: file.relative_path.clone(),
                label: file
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(file.relative_path.as_str())
                    .to_string(),
                path: file.relative_path.clone(),
                node_type: GraphNodeType::File,
                weight: file
                    .content
                    .as_ref()
                    .map(|content| content.lines().count())
                    .unwrap_or(1)
                    .max(1),
            })
            .collect::<Vec<_>>();

        let mut external_nodes = HashSet::new();
        let mut edges = Vec::new();

        for (source, imports) in &dependencies.imports {
            for reference in imports {
                if let Some(target) = &reference.resolved_path {
                    edges.push(GraphEdge {
                        source: source.clone(),
                        target: target.clone(),
                        kind: "internal".to_string(),
                    });
                } else {
                    external_nodes.insert(reference.specifier.clone());
                    edges.push(GraphEdge {
                        source: source.clone(),
                        target: reference.specifier.clone(),
                        kind: "external".to_string(),
                    });
                }
            }
        }

        for node in external_nodes {
            nodes.push(GraphNode {
                id: node.clone(),
                label: node.clone(),
                path: node,
                node_type: GraphNodeType::External,
                weight: 1,
            });
        }

        edges.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then(left.target.cmp(&right.target))
                .then(left.kind.cmp(&right.kind))
        });

        KnowledgeGraph { nodes, edges }
    }
}

#[derive(Default)]
struct ExtractedDependencies {
    imports: Vec<DependencyReference>,
    exports: Vec<String>,
}

fn count_directories(node: &FileTreeNode) -> usize {
    node.children
        .iter()
        .filter(|child| matches!(child.node_type, NodeType::Directory))
        .map(|child| 1 + count_directories(child))
        .sum()
}

fn sort_tree(node: &mut FileTreeNode) {
    node.children
        .sort_by(|left, right| match (&left.node_type, &right.node_type) {
            (NodeType::Directory, NodeType::File) => std::cmp::Ordering::Less,
            (NodeType::File, NodeType::Directory) => std::cmp::Ordering::Greater,
            _ => left.name.cmp(&right.name),
        });

    for child in &mut node.children {
        sort_tree(child);
    }
}

fn file_kind_label(file_type: &FileType) -> String {
    match file_type {
        FileType::SourceCode { .. } => "source".to_string(),
        FileType::Config => "config".to_string(),
        FileType::Documentation => "documentation".to_string(),
        FileType::Test => "test".to_string(),
        FileType::Asset => "asset".to_string(),
        FileType::Other => "other".to_string(),
    }
}

fn file_language(file_type: &FileType) -> Option<String> {
    match file_type {
        FileType::SourceCode { language } => Some(language.clone()),
        _ => None,
    }
}

fn normalize_path(input: &str) -> String {
    normalize_path_from_pathbuf(PathBuf::from(input))
}

fn normalize_path_from_pathbuf(path: PathBuf) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str().map(|value| value.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

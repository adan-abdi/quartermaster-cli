use crate::analyzer::{
    AnalysisResult, DependencyKind, FileTreeNode, GraphNodeType, NodeType, OverviewStats,
};
use crate::scanner::{FileInfo, FileType, RepoInfo, SourceType};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

pub const WORKSPACE_DIR_NAME: &str = ".quartermaster";
pub const WORKSPACE_VERSIONS_DIR_NAME: &str = "versions";
pub const DEV_DOCS_DIR_NAME: &str = "dev_docs";
pub const NOTES_DIR_NAME: &str = "notes";
pub const MANIFEST_FILE_NAME: &str = "manifest.json";
pub const CURRENT_VERSION_FILE_NAME: &str = "current.txt";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontMatter {
    pub id: String,
    pub title: String,
    pub code_path: String,
    pub doc_path: String,
    pub file_type: String,
    pub language: Option<String>,
    pub size: u64,
    pub lines_of_code: Option<usize>,
    pub generated_at: String,
    pub imports: Vec<String>,
    pub references: Vec<String>,
    pub referenced_by: Vec<String>,
    pub exports: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedWorkspace {
    pub root: PathBuf,
    pub current_root: PathBuf,
    pub manifest_path: PathBuf,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub generated_at: String,
    pub workspace_dir: String,
    pub repo: RepoManifest,
    pub overview: OverviewStats,
    pub code_tree: WorkspaceTreeNode,
    pub developer_docs_tree: WorkspaceTreeNode,
    pub docs: Vec<WorkspaceDoc>,
    pub notes: Vec<WorkspaceDoc>,
    pub graph: GraphManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoManifest {
    pub name: String,
    pub root_path: String,
    pub source_type: String,
    pub selected_roots: Vec<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTreeNode {
    pub name: String,
    pub path: String,
    pub node_type: String,
    pub children: Vec<WorkspaceTreeNode>,
    pub page_id: Option<String>,
    pub doc_relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDoc {
    pub id: String,
    pub title: String,
    pub path: String,
    pub doc_relative_path: String,
    pub description: String,
    pub file_type: String,
    pub language: Option<String>,
    pub size: u64,
    pub lines_of_code: Option<usize>,
    pub imports: Vec<String>,
    pub references: Vec<String>,
    pub referenced_by: Vec<String>,
    pub exports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphManifest {
    pub nodes: Vec<GraphNodeRecord>,
    pub edges: Vec<GraphEdgeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNodeRecord {
    pub id: String,
    pub label: String,
    pub path: String,
    pub node_type: String,
    pub weight: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdgeRecord {
    pub source: String,
    pub target: String,
    pub kind: String,
}

pub struct DocGenerator {
    workspace_dir_name: String,
}

impl DocGenerator {
    pub fn new() -> Self {
        Self {
            workspace_dir_name: WORKSPACE_DIR_NAME.to_string(),
        }
    }

    pub fn generate(
        &self,
        repo_info: &RepoInfo,
        analysis: &AnalysisResult,
    ) -> Result<GeneratedWorkspace> {
        let workspace_root = repo_info.path.join(&self.workspace_dir_name);
        let versions_root = workspace_root.join(WORKSPACE_VERSIONS_DIR_NAME);
        let notes_root = workspace_root.join(NOTES_DIR_NAME);
        let version_id = generate_version_id();
        let current_root = versions_root.join(&version_id);
        let dev_docs_root = current_root.join(DEV_DOCS_DIR_NAME);

        fs::create_dir_all(&workspace_root)?;
        fs::create_dir_all(&versions_root)?;
        fs::create_dir_all(&current_root)?;
        fs::create_dir_all(&dev_docs_root)?;
        fs::create_dir_all(&notes_root)?;

        let notes_readme_path = notes_root.join("README.md");
        if !notes_readme_path.exists() {
            fs::write(
                &notes_readme_path,
                "# Quartermaster Notes\n\nUse this space for working notes, follow-ups, and repo-specific context that should live beside the generated dev docs.\n",
            )?;
        }

        let docs = self.generate_dev_docs(repo_info, analysis, &dev_docs_root)?;
        let notes = self.collect_note_pages(&workspace_root)?;

        self.generate_workspace_readme(repo_info, analysis, &current_root)?;

        let doc_lookup = docs
            .iter()
            .map(|doc| (doc.path.clone(), doc.clone()))
            .collect::<HashMap<_, _>>();

        let manifest = WorkspaceManifest {
            generated_at: Utc::now().to_rfc3339(),
            workspace_dir: self.workspace_dir_name.clone(),
            repo: RepoManifest {
                name: repo_info.name.clone(),
                root_path: repo_info.path.display().to_string(),
                source_type: match &repo_info.source_type {
                    SourceType::GitHub { .. } => "github".to_string(),
                    SourceType::Local { .. } => "local".to_string(),
                },
                selected_roots: repo_info.selected_roots.clone(),
                branch: repo_info
                    .git_info
                    .as_ref()
                    .map(|git_info| git_info.branch.clone()),
                commit: repo_info
                    .git_info
                    .as_ref()
                    .map(|git_info| git_info.commit.clone()),
                remote_url: repo_info
                    .git_info
                    .as_ref()
                    .and_then(|git_info| git_info.remote_url.clone()),
            },
            overview: analysis.overview.clone(),
            code_tree: self.build_code_tree(&analysis.file_tree, &doc_lookup),
            developer_docs_tree: self.build_developer_docs_tree(&analysis.file_tree, &doc_lookup),
            docs,
            notes,
            graph: GraphManifest {
                nodes: analysis
                    .graph
                    .nodes
                    .iter()
                    .map(|node| GraphNodeRecord {
                        id: node.id.clone(),
                        label: node.label.clone(),
                        path: node.path.clone(),
                        node_type: match node.node_type {
                            GraphNodeType::File => "file".to_string(),
                            GraphNodeType::External => "external".to_string(),
                        },
                        weight: node.weight,
                    })
                    .collect(),
                edges: analysis
                    .graph
                    .edges
                    .iter()
                    .map(|edge| GraphEdgeRecord {
                        source: edge.source.clone(),
                        target: edge.target.clone(),
                        kind: edge.kind.clone(),
                    })
                    .collect(),
            },
        };

        let manifest_path = current_root.join(MANIFEST_FILE_NAME);
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
        fs::write(
            workspace_root.join(CURRENT_VERSION_FILE_NAME),
            format!("{version_id}\n"),
        )?;

        Ok(GeneratedWorkspace {
            root: workspace_root,
            current_root,
            manifest_path,
            version_id,
        })
    }

    fn generate_dev_docs(
        &self,
        repo_info: &RepoInfo,
        analysis: &AnalysisResult,
        dev_docs_root: &Path,
    ) -> Result<Vec<WorkspaceDoc>> {
        let mut docs = Vec::new();

        self.generate_directory_index(".", &analysis.file_tree, analysis, dev_docs_root)?;

        for file in &repo_info.files {
            if let Some(parent) = Path::new(&file.relative_path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(dev_docs_root.join(parent))?;
                }
            }

            let doc = self.generate_file_doc(file, analysis, dev_docs_root)?;
            docs.push(doc);
        }

        for directory in collect_directory_nodes(&analysis.file_tree) {
            if directory.path == "." {
                continue;
            }

            let directory_path = dev_docs_root.join(&directory.path);
            fs::create_dir_all(&directory_path)?;
            self.generate_directory_index(&directory.path, directory, analysis, dev_docs_root)?;
        }

        docs.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(docs)
    }

    fn generate_workspace_readme(
        &self,
        repo_info: &RepoInfo,
        analysis: &AnalysisResult,
        workspace_root: &Path,
    ) -> Result<()> {
        let readme = format!(
            "# Quartermaster Workspace\n\nGenerated on {} for `{}`.\n\n## Overview\n\n- Root: `{}`\n- Selected roots: {}\n- Total files: {}\n- Lines of code: {}\n- Stack: {}\n\n## Layout\n\n- `dev_docs/` mirrored developer docs with frontmatter and references\n- `notes/` writable notes for humans\n- `manifest.json` dashboard hydration source\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            repo_info.name,
            repo_info.path.display(),
            if repo_info.selected_roots.is_empty() {
                "all root entries".to_string()
            } else {
                repo_info.selected_roots.join(", ")
            },
            analysis.total_files,
            analysis.lines_of_code,
            analysis.tech_stack.join(", ")
        );

        fs::write(workspace_root.join("README.md"), readme)?;
        Ok(())
    }

    fn generate_directory_index(
        &self,
        path: &str,
        node: &FileTreeNode,
        analysis: &AnalysisResult,
        dev_docs_root: &Path,
    ) -> Result<()> {
        let relative_doc_path = doc_relative_path_for_directory(path);
        let doc_path = dev_docs_root.join(relative_doc_path.trim_start_matches("dev_docs/"));
        let directory_title = if path == "." {
            "Repository".to_string()
        } else {
            node.name.clone()
        };

        let mut content = format!("# {directory_title}\n\n");
        content.push_str(
            "_Directory note generated by Quartermaster. Add long-form documentation here._\n\n",
        );
        content.push_str("## Contents\n\n");

        for child in &node.children {
            match child.node_type {
                NodeType::Directory => {
                    content.push_str(&format!("- `{}`\n", child.path));
                }
                NodeType::File => {
                    content.push_str(&format!("- `{}`\n", child.path));
                }
            }
        }

        let imports = analysis
            .dependencies
            .internal_references
            .get(path)
            .cloned()
            .unwrap_or_default();

        let frontmatter = FrontMatter {
            id: Uuid::new_v4().to_string(),
            title: directory_title,
            code_path: path.to_string(),
            doc_path: relative_doc_path.clone(),
            file_type: "directory".to_string(),
            language: None,
            size: 0,
            lines_of_code: None,
            generated_at: Utc::now().to_rfc3339(),
            imports: imports.clone(),
            references: imports,
            referenced_by: Vec::new(),
            exports: Vec::new(),
            tags: vec!["directory".to_string()],
        };

        let full_content = format!(
            "---\n{}---\n\n{}",
            serde_yaml::to_string(&frontmatter)?,
            content
        );
        fs::write(doc_path, full_content)?;
        Ok(())
    }

    fn generate_file_doc(
        &self,
        file_info: &FileInfo,
        analysis: &AnalysisResult,
        dev_docs_root: &Path,
    ) -> Result<WorkspaceDoc> {
        let relative_doc_path = doc_relative_path_for_file(&file_info.relative_path);
        let doc_path = dev_docs_root.join(relative_doc_path.trim_start_matches("dev_docs/"));
        let imports = analysis
            .dependencies
            .imports
            .get(&file_info.relative_path)
            .cloned()
            .unwrap_or_default();
        let references = analysis
            .dependencies
            .internal_references
            .get(&file_info.relative_path)
            .cloned()
            .unwrap_or_default();
        let referenced_by = analysis
            .dependencies
            .dependents
            .get(&file_info.relative_path)
            .cloned()
            .unwrap_or_default();
        let exports = analysis
            .dependencies
            .exports
            .get(&file_info.relative_path)
            .cloned()
            .unwrap_or_default();
        let language = file_language(&file_info.file_type);
        let lines_of_code = file_info
            .content
            .as_ref()
            .map(|content| content.lines().count());
        let frontmatter = FrontMatter {
            id: Uuid::new_v4().to_string(),
            title: file_name_from_path(&file_info.relative_path),
            code_path: file_info.relative_path.clone(),
            doc_path: relative_doc_path.clone(),
            file_type: file_kind_label(&file_info.file_type),
            language: language.clone(),
            size: file_info.size,
            lines_of_code,
            generated_at: Utc::now().to_rfc3339(),
            imports: imports
                .iter()
                .map(|reference| reference.specifier.clone())
                .collect(),
            references: references.clone(),
            referenced_by: referenced_by.clone(),
            exports: exports.clone(),
            tags: build_tags(&file_info.file_type),
        };

        let mut content = format!("# {}\n\n", frontmatter.title);
        content.push_str("_Generated developer doc stub. Replace this body with LLM-authored or human-authored documentation._\n\n");
        content.push_str("## Context\n\n");
        content.push_str(&format!("- Source file: `{}`\n", file_info.relative_path));
        content.push_str(&format!("- File type: `{}`\n", frontmatter.file_type));
        content.push_str(&format!("- Size: `{}` bytes\n", file_info.size));

        if let Some(language) = &language {
            content.push_str(&format!("- Language: `{language}`\n"));
        }

        if let Some(lines_of_code) = lines_of_code {
            content.push_str(&format!("- Lines of code: `{lines_of_code}`\n"));
        }

        content.push_str("\n## Documentation\n\n");
        content.push_str("TBD.\n");

        content.push_str("\n---\n\n");
        content.push_str("## References\n\n");
        content.push_str(&format_markdown_list(
            "Code",
            &[file_info.relative_path.clone()],
        ));
        content.push_str(&format_markdown_list(
            "Imports",
            &imports
                .iter()
                .map(|reference| match reference.kind {
                    DependencyKind::Internal => reference
                        .resolved_path
                        .clone()
                        .unwrap_or_else(|| reference.specifier.clone()),
                    DependencyKind::External => reference.specifier.clone(),
                })
                .collect::<Vec<_>>(),
        ));
        content.push_str(&format_markdown_list("References", &references));
        content.push_str(&format_markdown_list("Referenced by", &referenced_by));
        content.push_str(&format_markdown_list("Exports", &exports));

        let full_content = format!(
            "---\n{}---\n\n{}",
            serde_yaml::to_string(&frontmatter)?,
            content
        );
        fs::write(&doc_path, full_content)?;

        Ok(WorkspaceDoc {
            id: frontmatter.id,
            title: frontmatter.title,
            path: file_info.relative_path.clone(),
            doc_relative_path: relative_doc_path,
            description: describe_file(file_info, lines_of_code),
            file_type: frontmatter.file_type,
            language,
            size: file_info.size,
            lines_of_code,
            imports: frontmatter.imports,
            references,
            referenced_by,
            exports,
        })
    }

    fn build_code_tree(
        &self,
        tree: &FileTreeNode,
        docs: &HashMap<String, WorkspaceDoc>,
    ) -> WorkspaceTreeNode {
        WorkspaceTreeNode {
            name: tree.name.clone(),
            path: tree.path.clone(),
            node_type: match tree.node_type {
                NodeType::Directory => "directory".to_string(),
                NodeType::File => "file".to_string(),
            },
            children: tree
                .children
                .iter()
                .map(|child| self.build_code_tree(child, docs))
                .collect(),
            page_id: docs.get(&tree.path).map(|doc| doc.id.clone()),
            doc_relative_path: docs
                .get(&tree.path)
                .map(|doc| doc.doc_relative_path.clone())
                .or_else(|| {
                    if matches!(tree.node_type, NodeType::Directory) {
                        Some(doc_relative_path_for_directory(&tree.path))
                    } else {
                        None
                    }
                }),
        }
    }

    fn build_developer_docs_tree(
        &self,
        tree: &FileTreeNode,
        docs: &HashMap<String, WorkspaceDoc>,
    ) -> WorkspaceTreeNode {
        WorkspaceTreeNode {
            name: DEV_DOCS_DIR_NAME.to_string(),
            path: DEV_DOCS_DIR_NAME.to_string(),
            node_type: "directory".to_string(),
            children: tree
                .children
                .iter()
                .map(|child| self.build_developer_docs_tree_node(child, docs))
                .collect(),
            page_id: None,
            doc_relative_path: Some(format!("{DEV_DOCS_DIR_NAME}/README.md")),
        }
    }

    fn build_developer_docs_tree_node(
        &self,
        node: &FileTreeNode,
        docs: &HashMap<String, WorkspaceDoc>,
    ) -> WorkspaceTreeNode {
        match node.node_type {
            NodeType::Directory => WorkspaceTreeNode {
                name: node.name.clone(),
                path: doc_relative_path_for_directory(&node.path),
                node_type: "directory".to_string(),
                children: node
                    .children
                    .iter()
                    .map(|child| self.build_developer_docs_tree_node(child, docs))
                    .collect(),
                page_id: None,
                doc_relative_path: Some(doc_relative_path_for_directory(&node.path)),
            },
            NodeType::File => {
                let doc = docs.get(&node.path);
                WorkspaceTreeNode {
                    name: format!("{}.md", node.name),
                    path: doc
                        .map(|doc| doc.doc_relative_path.clone())
                        .unwrap_or_else(|| doc_relative_path_for_file(&node.path)),
                    node_type: "file".to_string(),
                    children: Vec::new(),
                    page_id: doc.map(|doc| doc.id.clone()),
                    doc_relative_path: doc.map(|doc| doc.doc_relative_path.clone()),
                }
            }
        }
    }

    fn collect_note_pages(&self, workspace_root: &Path) -> Result<Vec<WorkspaceDoc>> {
        let notes_root = workspace_root.join(NOTES_DIR_NAME);
        let mut notes = Vec::new();

        for entry in WalkDir::new(&notes_root)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            if entry.file_type().is_dir() {
                continue;
            }

            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }

            let relative_path = match path.strip_prefix(workspace_root) {
                Ok(path) => normalize_path(path),
                Err(_) => continue,
            };

            let title = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("note")
                .replace('-', " ");

            let size = fs::metadata(path)?.len();
            let id = note_id_from_relative_path(&relative_path);

            notes.push(WorkspaceDoc {
                id,
                title,
                path: relative_path.clone(),
                doc_relative_path: relative_path,
                description: "Workspace note".to_string(),
                file_type: "note".to_string(),
                language: Some("Markdown".to_string()),
                size,
                lines_of_code: fs::read_to_string(path)
                    .ok()
                    .map(|content| content.lines().count()),
                imports: Vec::new(),
                references: Vec::new(),
                referenced_by: Vec::new(),
                exports: Vec::new(),
            });
        }

        notes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(notes)
    }
}

fn collect_directory_nodes<'a>(node: &'a FileTreeNode) -> Vec<&'a FileTreeNode> {
    let mut directories = Vec::new();

    if matches!(node.node_type, NodeType::Directory) {
        directories.push(node);
    }

    for child in &node.children {
        directories.extend(collect_directory_nodes(child));
    }

    directories
}

fn doc_relative_path_for_file(path: &str) -> String {
    format!("{DEV_DOCS_DIR_NAME}/{path}.md")
}

fn doc_relative_path_for_directory(path: &str) -> String {
    if path == "." {
        format!("{DEV_DOCS_DIR_NAME}/README.md")
    } else {
        format!("{DEV_DOCS_DIR_NAME}/{path}/README.md")
    }
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
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
        FileType::Documentation => Some("Markdown".to_string()),
        _ => None,
    }
}

fn build_tags(file_type: &FileType) -> Vec<String> {
    match file_type {
        FileType::SourceCode { language } => vec!["source".to_string(), language.to_lowercase()],
        FileType::Config => vec!["config".to_string()],
        FileType::Documentation => vec!["documentation".to_string()],
        FileType::Test => vec!["test".to_string()],
        FileType::Asset => vec!["asset".to_string()],
        FileType::Other => vec!["other".to_string()],
    }
}

fn describe_file(file_info: &FileInfo, lines_of_code: Option<usize>) -> String {
    match (&file_info.file_type, lines_of_code) {
        (FileType::SourceCode { language }, Some(lines_of_code)) => {
            format!("{language} source file with {lines_of_code} lines")
        }
        (FileType::Config, _) => "Configuration file".to_string(),
        (FileType::Documentation, _) => "Documentation file".to_string(),
        (FileType::Test, Some(lines_of_code)) => format!("Test file with {lines_of_code} lines"),
        (FileType::Asset, _) => "Static asset".to_string(),
        _ => "Project file".to_string(),
    }
}

fn format_markdown_list(title: &str, items: &[String]) -> String {
    let mut section = format!("### {title}\n\n");

    if items.is_empty() {
        section.push_str("- None\n\n");
        return section;
    }

    for item in items {
        section.push_str(&format!("- `{item}`\n"));
    }
    section.push('\n');
    section
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str().map(|value| value.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn generate_version_id() -> String {
    let now = Utc::now();
    let suffix = Uuid::new_v4().simple().to_string();
    format!("{}-{}", now.format("%Y%m%dT%H%M%SZ"), &suffix[..8])
}

pub fn note_id_from_relative_path(path: &str) -> String {
    format!("note:{}", path.trim_start_matches('/'))
}

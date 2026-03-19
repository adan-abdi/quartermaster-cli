use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use crossterm::{terminal::Clear, ExecutableCommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect};
use std::collections::HashSet;
use std::fs;
use std::io::{stdout, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

mod analyzer;
mod art;
mod generator;
mod scanner;
mod server;

use analyzer::{AnalysisResult, RepoAnalyzer};
use art::{display_anchor, display_logo, display_starfield, display_title};
use generator::{DocGenerator, GeneratedWorkspace, WORKSPACE_DIR_NAME};
use scanner::{PreparedRepo, RepoInfo, RepoScanner, RootEntry};
use server::launch_dashboard;

const DEFAULT_PORT: u16 = 4210;

#[derive(Parser)]
#[command(name = "quartermaster")]
#[command(about = "Generate repo docs and launch the Quartermaster dashboard")]
#[command(version = "1.1.0")]
#[command(author = "Quartermaster Team")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Chart a repository and open the dashboard
    #[command(alias = "analyze")]
    Chart {
        /// GitHub repository URL or local path. Defaults to the current directory in non-interactive mode.
        source: Option<String>,
        /// Skip gitignored files when scanning
        #[arg(long = "no-gitignore", default_value_t = false)]
        no_gitignore: bool,
        /// Root-level files or folders to include
        #[arg(long = "include-root", value_delimiter = ',')]
        include_roots: Vec<String>,
        /// Track ./.quartermaster in git
        #[arg(long, default_value_t = false)]
        track_workspace: bool,
        /// Do not launch the dashboard after generation
        #[arg(long, default_value_t = false)]
        no_open: bool,
        /// Skip interactive prompts and use defaults
        #[arg(long, default_value_t = false)]
        non_interactive: bool,
        /// Local dashboard port
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// Initialize Quartermaster defaults
    Init,
}

#[derive(Debug, Clone)]
struct AnalyzeOptions {
    source: String,
    respect_gitignore: bool,
    include_roots: Vec<String>,
    keep_workspace_untracked: bool,
    open_dashboard: bool,
    non_interactive: bool,
    port: u16,
}

#[derive(Default)]
struct PipelineState {
    repo_info: Option<RepoInfo>,
    analysis: Option<AnalysisResult>,
    workspace: Option<GeneratedWorkspace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PipelineStepId {
    Scan,
    Analyze,
    GenerateDocs,
}

struct PipelineStep {
    id: PipelineStepId,
    depends_on: &'static [PipelineStepId],
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    display_introduction()?;

    match cli.command {
        Some(Commands::Chart {
            source,
            no_gitignore,
            include_roots,
            track_workspace,
            no_open,
            non_interactive,
            port,
        }) => {
            let options = AnalyzeOptions {
                source: source.unwrap_or_else(|| ".".to_string()),
                respect_gitignore: !no_gitignore,
                include_roots,
                keep_workspace_untracked: !track_workspace,
                open_dashboard: !no_open,
                non_interactive,
                port,
            };
            analyze_repository(options)?;
        }
        Some(Commands::Init) => initialize_config()?,
        None => {
            let options = build_interactive_options()?;
            analyze_repository(options)?;
        }
    }

    Ok(())
}

fn display_introduction() -> Result<()> {
    stdout().execute(Clear(crossterm::terminal::ClearType::All))?;

    display_logo()?;
    thread::sleep(Duration::from_millis(500));

    display_starfield()?;
    thread::sleep(Duration::from_millis(300));

    display_title()?;
    println!();
    println!(
        "{}",
        "✦ Navigate the constellations of your codebase ✦".bright_cyan()
    );
    println!();
    println!(
        "{}",
        "Quartermaster scans your repository, creates versioned docs in ./.quartermaster,"
            .bright_black()
    );
    println!(
        "{}",
        "preserves your notes, and opens a dashboard that hydrates from the latest generated pass."
            .bright_black()
    );
    println!();
    let _ = display_anchor();

    Ok(())
}

fn build_interactive_options() -> Result<AnalyzeOptions> {
    let theme = ColorfulTheme::default();
    let source = Input::with_theme(&theme)
        .with_prompt("Repository source (local path or GitHub URL)")
        .default(".".to_string())
        .interact_text()?;

    let respect_gitignore = Confirm::with_theme(&theme)
        .with_prompt("Respect .gitignore while scanning?")
        .default(true)
        .interact()?;

    let track_workspace = Confirm::with_theme(&theme)
        .with_prompt("Track ./.quartermaster in git?")
        .default(false)
        .interact()?;

    Ok(AnalyzeOptions {
        source,
        respect_gitignore,
        include_roots: Vec::new(),
        keep_workspace_untracked: !track_workspace,
        open_dashboard: true,
        non_interactive: false,
        port: DEFAULT_PORT,
    })
}

fn analyze_repository(mut options: AnalyzeOptions) -> Result<()> {
    println!();
    println!("{}", "⚓ Charting your repository...".bright_blue().bold());
    println!();

    let scanner = RepoScanner::new(options.respect_gitignore);
    display_loading_animation("Charting course to repository")?;
    let prepared = scanner.prepare(&options.source)?;

    let root_entries = scanner.list_root_entries(&prepared.path, WORKSPACE_DIR_NAME)?;
    let selected_roots = resolve_selected_roots(&root_entries, &mut options)?;

    if options.keep_workspace_untracked {
        maybe_add_workspace_to_gitignore(&prepared.path, WORKSPACE_DIR_NAME)?;
    }

    let state = run_pipeline(&scanner, prepared, selected_roots)?;
    let repo_info = state
        .repo_info
        .ok_or_else(|| anyhow!("Repository scan did not complete"))?;
    let analysis = state
        .analysis
        .ok_or_else(|| anyhow!("Analysis did not complete"))?;
    let workspace = state
        .workspace
        .ok_or_else(|| anyhow!("Workspace generation did not complete"))?;

    print_summary(&repo_info, &analysis, &workspace);

    if options.open_dashboard {
        launch_dashboard(workspace.root.clone(), options.port)?;
    }

    Ok(())
}

fn resolve_selected_roots(
    root_entries: &[RootEntry],
    options: &mut AnalyzeOptions,
) -> Result<Vec<String>> {
    if !options.include_roots.is_empty() {
        return Ok(options.include_roots.clone());
    }

    let defaults = root_entries.iter().map(|_| true).collect::<Vec<_>>();

    if options.non_interactive || root_entries.is_empty() {
        options.include_roots = root_entries
            .iter()
            .map(|entry| entry.relative_path.clone())
            .collect();
        return Ok(options.include_roots.clone());
    }

    let labels = root_entries
        .iter()
        .map(|entry| {
            if entry.is_dir {
                format!("📁 {}", entry.relative_path)
            } else {
                format!("📄 {}", entry.relative_path)
            }
        })
        .collect::<Vec<_>>();

    let selected_indices = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Which root-level folders/files should Quartermaster generate docs for?")
        .items(&labels)
        .defaults(&defaults)
        .interact()?;

    options.include_roots = if selected_indices.is_empty() {
        root_entries
            .iter()
            .map(|entry| entry.relative_path.clone())
            .collect()
    } else {
        selected_indices
            .into_iter()
            .filter_map(|index| root_entries.get(index))
            .map(|entry| entry.relative_path.clone())
            .collect()
    };

    Ok(options.include_roots.clone())
}

fn run_pipeline(
    scanner: &RepoScanner,
    prepared: PreparedRepo,
    selected_roots: Vec<String>,
) -> Result<PipelineState> {
    let mut state = PipelineState::default();

    for step_id in topological_steps()? {
        match step_id {
            PipelineStepId::Scan => {
                display_loading_animation("Scanning repository tree")?;
                state.repo_info = Some(scanner.scan_prepared(
                    prepared.clone(),
                    &selected_roots,
                    WORKSPACE_DIR_NAME,
                )?);
            }
            PipelineStepId::Analyze => {
                display_loading_animation("Extracting stack, graph, and overview")?;
                let analyzer = RepoAnalyzer::new();
                let repo_info = state
                    .repo_info
                    .as_ref()
                    .ok_or_else(|| anyhow!("Scan step missing"))?;
                state.analysis = Some(analyzer.analyze(repo_info)?);
            }
            PipelineStepId::GenerateDocs => {
                display_loading_animation("Generating ./.quartermaster workspace")?;
                let generator = DocGenerator::new();
                let repo_info = state
                    .repo_info
                    .as_ref()
                    .ok_or_else(|| anyhow!("Scan step missing"))?;
                let analysis = state
                    .analysis
                    .as_ref()
                    .ok_or_else(|| anyhow!("Analyze step missing"))?;
                state.workspace = Some(generator.generate(repo_info, analysis)?);
            }
        }
    }

    Ok(state)
}

fn topological_steps() -> Result<Vec<PipelineStepId>> {
    let steps = vec![
        PipelineStep {
            id: PipelineStepId::Scan,
            depends_on: &[],
        },
        PipelineStep {
            id: PipelineStepId::Analyze,
            depends_on: &[PipelineStepId::Scan],
        },
        PipelineStep {
            id: PipelineStepId::GenerateDocs,
            depends_on: &[PipelineStepId::Analyze],
        },
    ];

    let mut resolved = Vec::new();
    let mut remaining = steps.iter().map(|step| step.id).collect::<HashSet<_>>();

    while !remaining.is_empty() {
        let next = steps
            .iter()
            .find(|step| {
                remaining.contains(&step.id)
                    && step
                        .depends_on
                        .iter()
                        .all(|dependency| resolved.contains(dependency))
            })
            .ok_or_else(|| anyhow!("Quartermaster pipeline contains a cycle"))?;

        remaining.remove(&next.id);
        resolved.push(next.id);
    }

    Ok(resolved)
}

fn maybe_add_workspace_to_gitignore(repo_root: &Path, workspace_dir_name: &str) -> Result<()> {
    let git_dir = repo_root.join(".git");
    if !git_dir.exists() {
        return Ok(());
    }

    let gitignore_path = repo_root.join(".gitignore");
    let entry = format!("/{workspace_dir_name}/");
    let current = fs::read_to_string(&gitignore_path).unwrap_or_default();

    if current.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }

    let separator = if current.is_empty() || current.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let updated = format!("{current}{separator}{entry}\n");
    fs::write(gitignore_path, updated)?;
    Ok(())
}

fn initialize_config() -> Result<()> {
    println!();
    println!(
        "{}",
        "⚙️  Quartermaster is ready to generate local workspaces."
            .bright_yellow()
            .bold()
    );
    println!(
        "{}",
        "Use `quartermaster chart`, `qm chart`, or just run `quartermaster` to start the interactive flow."
            .bright_white()
    );
    Ok(())
}

fn display_loading_animation(message: &str) -> Result<()> {
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    for index in 0..12 {
        print!(
            "\r{} {} {}",
            spinner_chars[index % spinner_chars.len()].bright_cyan(),
            message.bright_white(),
            ".".repeat((index / 2) % 4).bright_black()
        );
        stdout().flush()?;
        thread::sleep(Duration::from_millis(70));
    }

    print!("\r✅ {}", "Done!".bright_green());
    stdout().flush()?;
    print!(" {}", message.bright_white());
    stdout().flush()?;
    println!();
    Ok(())
}

fn print_summary(repo_info: &RepoInfo, analysis: &AnalysisResult, workspace: &GeneratedWorkspace) {
    println!();
    println!("{}", "🗺️  Analysis complete".bright_green().bold());
    println!(
        "{}",
        format!("📁 Repo: {}", repo_info.path.display()).bright_white()
    );
    println!(
        "{}",
        format!(
            "🧭 Scope: {}",
            if repo_info.selected_roots.is_empty() {
                "all root entries".to_string()
            } else {
                repo_info.selected_roots.join(", ")
            }
        )
        .bright_white()
    );
    println!(
        "{}",
        format!(
            "📊 Files: {} | LoC: {}",
            analysis.total_files, analysis.lines_of_code
        )
        .bright_white()
    );
    println!(
        "{}",
        format!("🌉 Stack: {}", analysis.tech_stack.join(", ")).bright_white()
    );
    println!(
        "{}",
        format!("📝 Workspace: {}", workspace.root.display()).bright_white()
    );
    println!(
        "{}",
        format!("🕰️  Active version: {}", workspace.version_id).bright_white()
    );

    if let Some(git_info) = &repo_info.git_info {
        println!(
            "{}",
            format!(
                "👥 Contributors inferred from git: {}",
                git_info.contributors.len()
            )
            .bright_white()
        );
    }

    println!();
}

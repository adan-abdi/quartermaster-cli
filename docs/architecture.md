# Quartermaster CLI Architecture

## Overview

Quartermaster is a standalone Rust CLI that analyzes a repository, writes a generated workspace to `./.quartermaster`, and serves a local browser dashboard for exploring the results.

The CLI has two goals:

1. produce a durable, versioned local documentation workspace
2. provide a self-contained dashboard experience without requiring a separate frontend checkout at runtime

## Runtime Model

At runtime, Quartermaster serves three categories of content:

1. embedded dashboard assets compiled from `cli/static`
2. generated workspace files from `./.quartermaster`
3. repository files from the active source checkout

This keeps the dashboard on a single local origin while still allowing it to inspect both generated docs and source files.

## Workspace Structure

Quartermaster writes output into a local workspace next to the analyzed repository:

```text
.quartermaster/
├── current.txt
├── notes/
└── versions/
    └── <version-id>/
        ├── manifest.json
        ├── README.md
        └── dev_docs/
```

Key directories and files:

- `notes/` stores durable human-authored notes
- `versions/<version-id>/` stores generated docs for a specific scan
- `current.txt` points the dashboard to the active generated version
- `manifest.json` describes the generated workspace tree, graph, and document metadata

## Embedded Dashboard

The published crate and installed binaries cannot rely on sibling paths such as `../dist`. To make the CLI portable, the dashboard bundle is copied into `cli/static` and embedded at compile time through `build.rs`.

That design provides:

- a self-contained runtime
- matching CLI and dashboard versions
- local-first behavior
- a simpler installation story for end users

## HTTP Surface

Quartermaster exposes a small local HTTP surface for the dashboard:

- `/dashboard/` serves the dashboard shell
- `/assets/*` serves embedded dashboard assets
- `/workspace/*` serves generated workspace content
- `/repo/*` serves repository files from the local checkout
- `/api/fs/create` allows the dashboard to create note files and folders

## Build and Packaging

The standalone crate packages:

- Rust sources
- crate metadata and license files
- embedded dashboard runtime assets
- public project documentation

The package manifest intentionally restricts included files so generated workspaces, build outputs, and unrelated repository files do not ship in the published crate.

## Artifact Sync Flow

The embedded dashboard bundle is refreshed from the main workspace build output:

1. build the dashboard bundle
2. run `./scripts/sync-dashboard-artifacts.sh`
3. verify package contents with `cargo package --list`
4. verify publish readiness with `cargo publish --dry-run`


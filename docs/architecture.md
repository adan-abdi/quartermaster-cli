# Quartermaster CLI Architecture

## Runtime Model

Quartermaster is a standalone Rust CLI that generates a local `./.quartermaster` workspace and serves a browser dashboard from a localhost HTTP server.

The runtime has three content sources:

1. Embedded dashboard assets compiled from `cli/static`
2. Generated workspace files under `./.quartermaster`
3. Repository files from the user's checkout

## Why The Dashboard Is Embedded

The published crate and installed binaries cannot rely on `../dist` or any sibling project layout at runtime. To keep installs portable, the dashboard bundle is copied into `cli/static` and embedded at compile time through `build.rs`.

That gives the CLI:

- offline dashboard availability
- matching CLI/UI versions
- one local origin for dashboard and file APIs
- no requirement for the dashboard source tree to be public

## Request Routing

- `/dashboard/` serves the embedded app shell
- `/assets/*` and other static runtime files are served from embedded assets
- `/workspace/*` serves generated Quartermaster workspace content
- `/repo/*` serves repository files from the current checkout
- `/api/fs/create` lets the dashboard create note files and folders

## Workspace Shape

Quartermaster writes:

- `./.quartermaster/versions/<version-id>/...` for generated docs
- `./.quartermaster/current.txt` for the active version pointer
- `./.quartermaster/notes/` for durable human-authored notes

## Release Workflow

1. Build the dashboard bundle in the source workspace.
2. Run `./scripts/sync-dashboard-artifacts.sh`.
3. Verify packaging with `cargo package --list`.
4. Publish the standalone CLI crate and release binaries.

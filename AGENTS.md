## Agent skills

### Issue tracker

Issues live in GitHub Issues for this repo (`hu3rror/opensteamtool-gui-rs`); use the `gh` CLI to create, read, comment, label, and close them. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical triage labels, each label string equal to its name: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: a root `CONTEXT.md` plus `docs/adr/` for decisions. See `docs/agents/domain.md`.

### Release

Push a `v*` tag to trigger `release.yml` (tag name is the version; main push only triggers cache-warm). Always create the tag with `git tag -a vX.Y.Z -m "..."` — this repo sets `tag.gpgSign=true`, so a bare `git tag vX.Y.Z` opens an editor and hangs in non-interactive shells. Local packaging: `tools/build-release.ps1 -Version <v>`.

# zte-f670l-toolkit

Reverse-engineering toolkit and a replacement admin web UI (`adminui/`) for the YYKF "ZTE F670L"
clone ONT. Start with `README.md`; device internals are in `docs/hardware.md`,
`adminui/notes/oss-protocol.md` and `adminui/notes/easymesh.md`.

This repository is meant to be public: never commit device dumps, configs, logs, real MAC
addresses, SSIDs or passwords (use RFC 7042 documentation MACs such as `00:00:5e:00:53:xx` in
tests). Owner-specific setup belongs outside the repo.

## Agent skills

### Issue tracker

Issues/tickets live in this repo's GitHub Issues, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage labels, used as-is (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` + `docs/adr/` at the repo root (created lazily as terms/decisions get resolved). See `docs/agents/domain.md`.

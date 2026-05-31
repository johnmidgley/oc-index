#!/usr/bin/env bash
# Launch Claude Code inside the dev container defined by .devcontainer/devcontainer.json,
# using the official Dev Containers CLI (https://github.com/devcontainers/cli).
#
# Unlike a hand-rolled `docker run` wrapper, this delegates ALL container
# lifecycle and security to the spec: the Dockerfile + devcontainer.json build a
# hardened image, and init-firewall.sh installs a default-deny network firewall
# on container start. That firewall (plus the non-root `vscode` user) is the
# sandbox — which is what makes --dangerously-skip-permissions defensible here.
#
# Credentials are NOT copied from the host. Log in once inside the container with
# `claude` (or `/login`); the ~/.claude config dir is a persistent volume, so the
# login survives rebuilds.
#
# Works on Linux and macOS. Extra args pass through to claude.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

command -v docker >/dev/null 2>&1 \
    || die "docker is required. Install from https://docs.docker.com/get-docker/"
docker info >/dev/null 2>&1 \
    || die "Docker daemon isn't reachable. Start Docker Desktop / the docker service and retry."
command -v devcontainer >/dev/null 2>&1 \
    || die "The Dev Containers CLI is required. Install with: npm install -g @devcontainers/cli"

# Build and start the container per devcontainer.json. Idempotent: reuses the
# existing container when one is already up, and runs the firewall postStartCommand.
devcontainer up --workspace-folder "$PROJECT_DIR"

# Open an interactive Claude Code session as the container's remoteUser (vscode).
# The container is the sandbox, so we skip per-tool permission prompts by default.
exec devcontainer exec --workspace-folder "$PROJECT_DIR" \
    claude --dangerously-skip-permissions "$@"

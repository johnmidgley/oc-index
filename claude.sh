#!/usr/bin/env bash
# Launch Claude Code inside a docker container described by .devcontainer/devcontainer.json.
#
# The script:
#   - Reads image / remoteUser / workspaceFolder / mounts from devcontainer.json
#   - Bind-mounts the project, the host's `claude` binary, and ~/.claude/.credentials.json
#   - Reuses a long-lived container so subsequent invocations are fast
#
# Companion: claude.ps1 (PowerShell, same behavior for Windows).

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEVCONTAINER_JSON="$PROJECT_DIR/.devcontainer/devcontainer.json"
CONTAINER_NAME="oci-claude"

die()  { printf 'error: %s\n' "$*" >&2; exit 1; }
note() { printf '%s\n' "$*" >&2; }

# ---------- Dependency checks ----------

detect_pm() {
    if command -v apt-get >/dev/null 2>&1; then echo "sudo apt-get install -y"
    elif command -v dnf     >/dev/null 2>&1; then echo "sudo dnf install -y"
    elif command -v pacman  >/dev/null 2>&1; then echo "sudo pacman -S --noconfirm"
    elif command -v brew    >/dev/null 2>&1; then echo "brew install"
    fi
}

prompt_install() {
    local pkg="$1" install_cmd="$2"
    note "Required tool '$pkg' is missing."
    read -rp "Run '$install_cmd $pkg' now? [y/N] " ans
    [[ "$ans" =~ ^[Yy]$ ]] || return 1
    eval "$install_cmd $pkg"
}

# Small tool that maps cleanly to a single package across managers.
# Auto-install is offered when a package manager is detected.
need_small_tool() {
    local tool="$1" install_hint="$2"
    command -v "$tool" >/dev/null 2>&1 && return 0
    local pm; pm="$(detect_pm || true)"
    if [[ -n "$pm" ]] && prompt_install "$tool" "$pm"; then return 0; fi
    die "$tool is required. $install_hint"
}

# Heavy tool with OS-specific installer flow — never auto-install.
need_tool_manual() {
    local tool="$1" install_hint="$2"
    command -v "$tool" >/dev/null 2>&1 && return 0
    die "$tool is required. $install_hint"
}

need_small_tool  jq     "Install jq manually for your platform."
need_tool_manual docker "Install from https://docs.docker.com/get-docker/ (on Debian/Ubuntu, 'sudo apt-get install docker.io' also works)."

if ! docker info >/dev/null 2>&1; then
    die "Docker daemon isn't reachable. Start it (e.g. 'sudo systemctl start docker') or add yourself to the docker group ('sudo usermod -aG docker \$USER' then log out/in)."
fi

CLAUDE_BIN="$(command -v claude 2>/dev/null || true)"
[[ -n "$CLAUDE_BIN" ]] || die "claude not found on PATH. Install from https://claude.com/claude-code"
# Resolve any symlink so the bind mount targets the real binary.
CLAUDE_BIN="$(readlink -f "$CLAUDE_BIN")"

CREDS="$HOME/.claude/.credentials.json"
CLAUDE_JSON="$HOME/.claude.json"
[[ -f "$CREDS" ]] || die "No credentials at $CREDS. Run 'claude' once on the host to log in."
[[ -f "$CLAUDE_JSON" ]] || die "No $CLAUDE_JSON on host. Run 'claude' once on the host to populate account state."

# ---------- Parse devcontainer.json ----------

# Strip line comments (// ...) — devcontainer.json is JSONC.
read_dc() { sed -e 's|//[^"]*$||' "$DEVCONTAINER_JSON" | jq -r "$1"; }

expand_env() {
    local s="$1" var val
    while [[ "$s" =~ \$\{localEnv:([A-Za-z_][A-Za-z0-9_]*)\} ]]; do
        var="${BASH_REMATCH[1]}"
        val="${!var:-}"
        s="${s//\$\{localEnv:$var\}/$val}"
    done
    printf '%s' "$s"
}

IMAGE="$(read_dc '.image // empty')"
[[ -n "$IMAGE" ]] || die "'image' missing in devcontainer.json"

REMOTE_USER="$(read_dc '.remoteUser // "root"')"
WORKSPACE="$(read_dc '.workspaceFolder // empty')"
[[ -n "$WORKSPACE" ]] || WORKSPACE="/workspaces/$(basename "$PROJECT_DIR")"

# ---------- Build docker run args ----------

mount_args=()

# Mounts declared in devcontainer.json (source=X,target=Y,type=bind[,readonly])
while IFS= read -r m; do
    [[ -z "$m" || "$m" == "null" ]] && continue
    m="$(expand_env "$m")"
    src="$(printf '%s\n' "$m" | tr ',' '\n' | sed -n 's/^source=//p')"
    tgt="$(printf '%s\n' "$m" | tr ',' '\n' | sed -n 's/^target=//p')"
    ro=""; [[ "$m" == *readonly* ]] && ro=":ro"
    [[ -n "$src" && -n "$tgt" ]] && mount_args+=(-v "$src:$tgt$ro")
done < <(read_dc '.mounts // [] | .[]')

# Claude-specific mounts added by this script (not in devcontainer.json).
# Credentials / .claude.json are NOT bind-mounted — Claude writes them with
# atomic rename, which fails on single-file bind mounts. We `docker cp` them
# instead (see below), so the container has its own writable copies.
mount_args+=(-v "$PROJECT_DIR:$WORKSPACE")
mount_args+=(-v "$CLAUDE_BIN:/usr/local/bin/claude:ro")

# ---------- Container lifecycle ----------

if docker ps --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; then
    :  # already running
elif docker ps -a --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; then
    docker start "$CONTAINER_NAME" >/dev/null
else
    note "Pulling $IMAGE..."
    docker pull "$IMAGE" >/dev/null
    note "Starting container '$CONTAINER_NAME'..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --user "$REMOTE_USER" \
        -w "$WORKSPACE" \
        "${mount_args[@]}" \
        "$IMAGE" \
        sleep infinity >/dev/null
fi

# Sync host credentials into the container (overwrites any container-side state).
# Done on every invocation so re-auth on the host propagates to the next session.
# `docker cp` runs as root inside the container; chown after so $REMOTE_USER can read/write.
docker exec -u root "$CONTAINER_NAME" mkdir -p "/home/$REMOTE_USER/.claude"
docker cp "$CREDS"       "$CONTAINER_NAME:/home/$REMOTE_USER/.claude/.credentials.json"
docker cp "$CLAUDE_JSON" "$CONTAINER_NAME:/home/$REMOTE_USER/.claude.json"
docker exec -u root "$CONTAINER_NAME" chown -R "$REMOTE_USER:$REMOTE_USER" \
    "/home/$REMOTE_USER/.claude" "/home/$REMOTE_USER/.claude.json"

# Propagate host git identity so commits made inside the container have an
# author. `git config --get` returns non-zero when unset; we skip in that case
# rather than fail, and warn if neither is set (commits will be rejected).
HOST_GIT_NAME="$(git config --global --get user.name 2>/dev/null || true)"
HOST_GIT_EMAIL="$(git config --global --get user.email 2>/dev/null || true)"
if [[ -n "$HOST_GIT_NAME" ]]; then
    docker exec -u "$REMOTE_USER" "$CONTAINER_NAME" \
        git config --global user.name "$HOST_GIT_NAME"
fi
if [[ -n "$HOST_GIT_EMAIL" ]]; then
    docker exec -u "$REMOTE_USER" "$CONTAINER_NAME" \
        git config --global user.email "$HOST_GIT_EMAIL"
fi
if [[ -z "$HOST_GIT_NAME" || -z "$HOST_GIT_EMAIL" ]]; then
    note "warning: host git user.name/user.email not fully set; commits inside the container may be rejected. Configure with 'git config --global user.name/user.email' on the host."
fi

# Run with --dangerously-skip-permissions by default: the container itself is
# the sandbox, so prompting for individual tool approvals adds friction without
# meaningful protection. Any user-provided args still come through via "$@".
exec docker exec -it "$CONTAINER_NAME" claude --dangerously-skip-permissions "$@"

# Rust dev container hardened with Anthropic's Claude Code sandbox model:
# a default-deny network firewall (init-firewall.sh) + non-root user are the
# security boundary, which is what makes running Claude with
# --dangerously-skip-permissions inside here defensible.
#
# Adapted from https://github.com/anthropics/claude-code/tree/main/.devcontainer
# onto the Rust base image (the upstream reference uses FROM node:20).
FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

ARG TZ
ENV TZ="$TZ"

ARG CLAUDE_CODE_VERSION=latest
ARG GIT_DELTA_VERSION=0.18.2
ARG NODE_MAJOR=20
ARG USERNAME=vscode

# Install the firewall tooling (iptables/ipset/aggregate/dnsutils/jq) plus Node
# (needed for the Claude Code npm package). Most base utilities already exist in
# the Rust image; re-listing them is harmless and keeps this self-contained.
RUN apt-get update && apt-get install -y --no-install-recommends \
    less \
    procps \
    sudo \
    fzf \
    zsh \
    man-db \
    unzip \
    gnupg2 \
    iptables \
    ipset \
    iproute2 \
    dnsutils \
    aggregate \
    jq \
    nano \
    vim \
    curl \
    wget \
    ca-certificates \
  && curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
  && apt-get install -y --no-install-recommends nodejs \
  && apt-get clean && rm -rf /var/lib/apt/lists/*

# git-delta for nicer diffs (matches the upstream reference).
RUN ARCH=$(dpkg --print-architecture) && \
  curl -fsSL -o /tmp/git-delta.deb \
    "https://github.com/dandavison/delta/releases/download/${GIT_DELTA_VERSION}/git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" && \
  dpkg -i /tmp/git-delta.deb && \
  rm /tmp/git-delta.deb

# npm global prefix the non-root user can write to.
RUN mkdir -p /usr/local/share/npm-global && \
  chown -R $USERNAME:$USERNAME /usr/local/share/npm-global

# Persist bash history across rebuilds (see the /commandhistory volume mount).
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
  && mkdir /commandhistory \
  && touch /commandhistory/.bash_history \
  && chown -R $USERNAME /commandhistory

# Claude config dir (mounted as a persistent volume so logins survive rebuilds).
RUN mkdir -p /home/$USERNAME/.claude && \
  chown -R $USERNAME:$USERNAME /home/$USERNAME/.claude

# Signal to tooling that this is a dev container.
ENV DEVCONTAINER=true
ENV NPM_CONFIG_PREFIX=/usr/local/share/npm-global
ENV PATH=$PATH:/usr/local/share/npm-global/bin
ENV SHELL=/bin/zsh
ENV EDITOR=nano
ENV VISUAL=nano

# Install Claude Code as the non-root user into the shared npm prefix.
USER $USERNAME
RUN npm install -g @anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}

# Copy and set up the firewall script. It must run as root (it manipulates
# iptables), so allow the non-root user to invoke just this one script via sudo
# without a password — that is what postStartCommand calls.
COPY init-firewall.sh /usr/local/bin/
USER root
RUN chmod +x /usr/local/bin/init-firewall.sh && \
  echo "$USERNAME ALL=(root) NOPASSWD: /usr/local/bin/init-firewall.sh" > /etc/sudoers.d/$USERNAME-firewall && \
  chmod 0440 /etc/sudoers.d/$USERNAME-firewall
USER $USERNAME

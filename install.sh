#!/usr/bin/env sh
set -e
TARGET="${HOME}/.local/bin"
REPO="infraflakes/kiru"

# Snag the clean tag string straight from the GitHub API JSON output using native sed
TAG=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": "\(.*\)".*/\1/p')

if [ -z "$TAG" ]; then
    echo "Error: Failed to fetch the latest release tag from GitHub." >&2
    exit 1
fi

mkdir -p "$TARGET"
echo "Fetching kc $TAG..."

curl -L -s -o "$TARGET/kc" "https://github.com/${REPO}/releases/download/${TAG}/kc-${TAG}-linux-x86_64"
chmod +x "$TARGET/kc"
echo "Installed kc $TAG to $TARGET/kc"

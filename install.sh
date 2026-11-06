#!/usr/bin/env bash
set -euo pipefail

REPO="Praveensenpai/seedr-relay"
BIN_DIR="${HOME}/.local/bin"
mkdir -p "${BIN_DIR}"

echo "Fetching latest seedr-relay release..."
LATEST_TAG=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep -Po '"tag_name": "\K[^"]*')
if [ -z "${LATEST_TAG}" ]; then
  echo "Error: Could not find latest release tag."
  exit 1
fi

ARCH="x86_64-unknown-linux-gnu"
URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/seedr-relay-${ARCH}.tar.gz"

echo "Downloading seedr-relay ${LATEST_TAG} for ${ARCH}..."
TMP_DIR=$(mktemp -d)
trap 'rm -rf "${TMP_DIR}"' EXIT

curl -fsSL "${URL}" -o "${TMP_DIR}/seedr-relay.tar.gz"
tar -xzf "${TMP_DIR}/seedr-relay.tar.gz" -C "${TMP_DIR}"
install -m 755 "${TMP_DIR}/seedr-relay" "${BIN_DIR}/seedr-relay"

echo "✔ Successfully installed seedr-relay to ${BIN_DIR}/seedr-relay"
seedr-relay --version || true

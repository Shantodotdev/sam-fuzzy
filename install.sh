#!/usr/bin/env bash
set -e

# SAM-FUZZY Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/Shantodotdev/sam-fuzzy/main/install.sh | bash

REPO="Shantodotdev/sam-fuzzy"
GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"

echo "==> Detecting platform architecture..."
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux*)
    case "$ARCH" in
      x86_64) TARGET="sam-fuzzy-linux-x86_64" ;;
      *) echo "Error: Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Darwin*)
    case "$ARCH" in
      arm64|aarch64) TARGET="sam-fuzzy-macos-arm64" ;;
      *) echo "Error: Only Apple Silicon (M1/M2/M3/M4) is supported on macOS." >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Error: Unsupported operating system: $OS" >&2
    echo "On Windows, please download sam-fuzzy-windows-x86_64.exe from: https://github.com/${REPO}/releases" >&2
    exit 1
    ;;
esac

echo "==> Fetching latest release info for ${TARGET}..."
LATEST_RELEASE_JSON="$(curl -sSL -H "Accept: application/vnd.github.v3+json" "$GITHUB_API")"

DOWNLOAD_URL="$(echo "$LATEST_RELEASE_JSON" | grep "browser_download_url" | grep "${TARGET}\"" | cut -d '"' -f 4 | head -n 1)"

if [ -z "$DOWNLOAD_URL" ]; then
  TAG="$(echo "$LATEST_RELEASE_JSON" | grep '"tag_name":' | cut -d '"' -f 4 | head -n 1)"
  if [ -z "$TAG" ]; then
    TAG="v0.1.0"
  fi
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${TARGET}"
fi

INSTALL_DIR="/usr/local/bin"
if [ ! -w "$INSTALL_DIR" ]; then
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

echo "==> Downloading sam-fuzzy to ${INSTALL_DIR}/sam-fuzzy..."
curl -fsSL "$DOWNLOAD_URL" -o "${INSTALL_DIR}/sam-fuzzy"
chmod +x "${INSTALL_DIR}/sam-fuzzy"

echo ""
echo "✨ sam-fuzzy installed successfully to ${INSTALL_DIR}/sam-fuzzy!"
echo ""
echo "To launch the explorer, simply run:"
echo "    sam-fuzzy"
echo ""

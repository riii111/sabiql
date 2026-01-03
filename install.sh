#!/bin/sh
set -e

REPO="riii111/sabiql"
BINARY_NAME="sabiql"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        darwin) OS="apple-darwin" ;;
        linux) OS="unknown-linux-gnu" ;;
        *)
            echo "Error: Unsupported OS: $OS"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *)
            echo "Error: Unsupported architecture: $ARCH"
            exit 1
            ;;
    esac

    PLATFORM="${ARCH}-${OS}"
}

get_latest_version() {
    curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/'
}

verify_checksum() {
    local file="$1"
    local checksum_file="$2"

    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum -c "$checksum_file" > /dev/null 2>&1
    elif command -v shasum > /dev/null 2>&1; then
        shasum -a 256 -c "$checksum_file" > /dev/null 2>&1
    else
        echo "Warning: No checksum tool available, skipping verification"
        return 0
    fi
}

install() {
    detect_platform
    VERSION=$(get_latest_version)

    if [ -z "$VERSION" ]; then
        echo "Error: Could not determine latest version"
        exit 1
    fi

    echo "Installing ${BINARY_NAME} ${VERSION} for ${PLATFORM}..."

    ARCHIVE_NAME="${BINARY_NAME}-${PLATFORM}.tar.gz"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_NAME}"
    CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

    TEMP_DIR=$(mktemp -d)
    TEMP_FILE="${TEMP_DIR}/${ARCHIVE_NAME}"
    CHECKSUM_FILE="${TEMP_DIR}/${ARCHIVE_NAME}.sha256"

    echo "Downloading from ${DOWNLOAD_URL}..."
    curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_FILE"
    curl -fsSL "$CHECKSUM_URL" -o "$CHECKSUM_FILE"

    echo "Verifying checksum..."
    cd "$TEMP_DIR"
    if ! verify_checksum "$TEMP_FILE" "$CHECKSUM_FILE"; then
        echo "Error: Checksum verification failed!"
        rm -rf "$TEMP_DIR"
        exit 1
    fi
    echo "Checksum verified."

    echo "Extracting..."
    tar -xzf "$TEMP_FILE" -C "$TEMP_DIR"

    echo "Installing to ${INSTALL_DIR}..."
    mkdir -p "$INSTALL_DIR"
    mv "${TEMP_DIR}/${BINARY_NAME}" "$INSTALL_DIR/"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    rm -rf "$TEMP_DIR"

    echo ""
    echo "Successfully installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
    echo ""

    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo "Add the following to your shell profile:"
            echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
            echo ""
            ;;
    esac

    echo "Run '${BINARY_NAME} --help' to get started."
}

install

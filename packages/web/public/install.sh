#!/bin/sh

set -eu

REPO="${SUPERMANAGER_INSTALL_REPO:-Sofianel5/supermanager}"
VERSION="${SUPERMANAGER_INSTALL_VERSION:-latest}"
INSTALL_DIR="${SUPERMANAGER_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="supermanager"
CHECKSUM_FILE="supermanager-checksums.txt"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os="apple-darwin" ;;
    Linux) os="unknown-linux-gnu" ;;
    *)
      echo "unsupported operating system: $os" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    arm64|aarch64) arch="aarch64" ;;
    x86_64|amd64) arch="x86_64" ;;
    *)
      echo "unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  if [ "$os" = "unknown-linux-gnu" ] && [ "$arch" = "aarch64" ]; then
    echo "linux aarch64 releases are not published yet" >&2
    exit 1
  fi

  printf '%s-%s' "$arch" "$os"
}

download() {
  url="$1"
  destination="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$destination"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -qO "$destination" "$url"
    return
  fi

  echo "missing required command: curl or wget" >&2
  exit 1
}

verify_checksum() {
  archive_path="$1"
  checksum_path="$2"
  archive_name="$(basename "$archive_path")"
  expected="$(awk -v name="$archive_name" '$2 == name { print $1 }' "$checksum_path")"

  if [ -z "$expected" ]; then
    echo "missing checksum for ${archive_name}" >&2
    exit 1
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$archive_path" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$archive_path" | awk '{print $1}')"
  else
    echo "missing required command: sha256sum or shasum" >&2
    exit 1
  fi

  if [ "$expected" != "$actual" ]; then
    echo "checksum mismatch for ${archive_name}" >&2
    exit 1
  fi
}

main() {
  need_cmd uname
  need_cmd tar
  need_cmd mktemp
  need_cmd grep
  need_cmd awk

  target="$(detect_target)"
  asset_name="${BIN_NAME}-${target}.tar.gz"

  if [ "$VERSION" = "latest" ]; then
    release_base_url="https://github.com/${REPO}/releases/latest/download"
  else
    release_base_url="https://github.com/${REPO}/releases/download/${VERSION}"
  fi

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  archive_path="${tmp_dir}/${asset_name}"
  checksum_path="${tmp_dir}/${CHECKSUM_FILE}"

  echo "==> Downloading ${asset_name}"
  download "${release_base_url}/${asset_name}" "$archive_path"
  download "${release_base_url}/${CHECKSUM_FILE}" "$checksum_path"

  echo "==> Verifying checksum"
  verify_checksum "$archive_path" "$checksum_path"

  mkdir -p "$INSTALL_DIR"
  tar -xzf "$archive_path" -C "$tmp_dir"
  install_path="${INSTALL_DIR}/${BIN_NAME}"
  mv "${tmp_dir}/${BIN_NAME}" "$install_path"
  chmod +x "$install_path"

  echo "==> Installed ${BIN_NAME} to ${install_path}"

  case ":$PATH:" in
    *:"$INSTALL_DIR":*)
      ;;
    *)
      echo "==> Add ${INSTALL_DIR} to your PATH if it is not already available."
      ;;
  esac

  echo "==> Run '${BIN_NAME} --version' to confirm the install."
}

main "$@"

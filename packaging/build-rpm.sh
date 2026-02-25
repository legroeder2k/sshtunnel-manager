#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "${SCRIPT_DIR}/.." && pwd)
SPEC_FILE="${SCRIPT_DIR}/sshtunnel-manager.spec"
RPMTOP="${SCRIPT_DIR}/rpmbuild"
OUTDIR_DEFAULT="${RPMTOP}/RPMS"

VERSION="0.1.0"
RELEASE="1"
OUTDIR="${OUTDIR_DEFAULT}"
NO_BUILD=0

usage() {
  cat <<USAGE
Usage: $(basename "$0") [options]

Build a Fedora RPM for sshtunnel-manager using packaging/sshtunnel-manager.spec.

Options:
  --version <ver>   RPM Version (default: ${VERSION})
  --release <rel>   RPM Release without dist tag (default: ${RELEASE})
  --outdir <dir>    Copy built RPMs into this directory after build
  --no-build        Only create source tarball/spec in rpmbuild tree
  -h, --help        Show this help

Notes:
- User systemd unit files are packaged system-wide into /usr/lib/systemd/user.
- RPM packages must not copy user units into ~/.config/systemd/user for each user.
USAGE
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    return 1
  }
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --release)
      RELEASE="$2"
      shift 2
      ;;
    --outdir)
      OUTDIR="$2"
      shift 2
      ;;
    --no-build)
      NO_BUILD=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_cmd tar
require_cmd rpmbuild

mkdir -p "${RPMTOP}/"{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

TARBALL="${RPMTOP}/SOURCES/sshtunnel-manager-${VERSION}.tar.gz"
SPEC_DST="${RPMTOP}/SPECS/sshtunnel-manager.spec"

rm -f -- "${TARBALL}"
cp -f -- "${SPEC_FILE}" "${SPEC_DST}"

# Build from the current working tree (including untracked files), excluding build output and VCS dirs.
tar \
  --exclude='.git' \
  --exclude='target' \
  --exclude='packaging/rpmbuild' \
  -C "${REPO_ROOT}" \
  -czf "${TARBALL}" \
  --transform "s#^#sshtunnel-manager-${VERSION}/#" \
  .

echo "Prepared source tarball: ${TARBALL}"
echo "Spec copied to: ${SPEC_DST}"

if [[ ${NO_BUILD} -eq 1 ]]; then
  exit 0
fi

rpmbuild -bb \
  --define "_topdir ${RPMTOP}" \
  --define "pkg_version ${VERSION}" \
  --define "pkg_release ${RELEASE}" \
  "${SPEC_DST}"

mkdir -p "${OUTDIR}"
find "${RPMTOP}/RPMS" -type f -name '*.rpm' -print -exec cp -f {} "${OUTDIR}" \;

echo
echo "RPM build complete. Copied packages to: ${OUTDIR}"
echo "Post-install (per user) enable command:"
echo "  systemctl --user enable --now sshtunnel-backendd.service"
echo
echo "Optional admin default for future logins (system-wide user enable):"
echo "  systemctl --global enable sshtunnel-backendd.service"

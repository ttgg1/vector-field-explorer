#!/usr/bin/env bash
# Install and validate the AppImage's system-library copyright notices (ticket 19).
#
# The AppImage bundles non-graphics shared libraries. Every bundled library must
# ship its Debian/Ubuntu package copyright, and every copyright that references
# /usr/share/common-licenses/... must ship that licence file too.
#
# Usage:
#   scripts/package-notices.sh install <appdir> [--sysroot <root>]
#   scripts/package-notices.sh validate <extracted-appimage-root>
#
# `install` derives the library list from the AppDir, resolves each library with
# `dpkg -S` (which is looked up on PATH, so tests can inject a fake), copies the
# owning package's copyright into
# usr/share/doc/vector-field-explorer/system-libraries/, and copies referenced
# common-licence files into the common-licenses/ child directory.
#
# `--sysroot` prefixes the host paths used for dpkg documentation and
# common-licences. It exists for tests and chroot-style validation; production
# packaging uses the host root.
#
# `validate` runs on the extracted AppImage and fails if a notice is missing or
# a common-licence reference does not resolve inside the image.
#
# Requirements: bash, coreutils, findutils, grep, sed, awk, dpkg (install only).

set -euo pipefail

common_ref_re='/usr/share/common-licenses/[A-Za-z0-9_+-]+([.][A-Za-z0-9_+-]+)*'

usage() {
  echo "usage: scripts/package-notices.sh install <appdir> [--sysroot <root>]" >&2
  echo "       scripts/package-notices.sh validate <extracted-appimage-root>" >&2
  exit 2
}

fail() {
  echo "::error::$*" >&2
  exit 1
}

mode="${1:-}"
[ -n "$mode" ] || usage
shift
target="${1:-}"
[ -n "$target" ] || usage
shift

sysroot=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --sysroot)
      [ "$#" -ge 2 ] || usage
      sysroot="${2%/}"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[ -d "$target" ] || fail "${target} is not a directory"

if [ "$mode" = "validate" ]; then
  [ -z "$sysroot" ] || fail "--sysroot is only valid for install"
fi

notice_dir="${target}/usr/share/doc/vector-field-explorer/system-libraries"
common_license_dir="${notice_dir}/common-licenses"

if [ "$mode" = "install" ]; then
  mkdir -p "$notice_dir" "$common_license_dir"
  printf 'Bundled library\tDebian/Ubuntu package\n' > "${notice_dir}/SOURCES.txt"

  libs_file="$(mktemp "${TMPDIR:-/tmp}/vfe-notice-libs.XXXXXX")"
  trap 'rm -f "$libs_file"' EXIT
  find "${target}/usr/lib" -maxdepth 2 \( -type f -o -type l \) \
    -name '*.so*' -printf '%f\n' | sort -u > "$libs_file"

  # AppDir-relative paths, for an actionable failure message.
  bundled_paths() {
    find "${target}/usr/lib" -maxdepth 2 \( -type f -o -type l \) \
      -name "$1" -print \
      | sed "s#^${target}/##" | sort | tr '\n' ' '
  }

  missing_notice=0
  while IFS= read -r lib; do
    [ -n "$lib" ] || continue

    # A path can be owned by more than one package (for example the runtime
    # package and a -dev or -dbg package), and dpkg reports architecture-
    # qualified names. Drop diversion bookkeeping lines, collect every distinct
    # owner, and prefer a runtime package instead of taking the first line.
    ownership="$(dpkg -S "*/${lib}" 2>/dev/null || true)"
    mapfile -t owners < <(
      printf '%s\n' "$ownership" \
        | grep -v -E '^(diversion by|locally diverted)' \
        | awk -F': ' 'NF >= 2 { sub(/:[^:]*$/, "", $1); print $1 }' \
        | sed '/^$/d' | sort -u || true
    )
    runtime_owners=()
    for candidate in "${owners[@]}"; do
      case "$candidate" in
        *-dev | *-dbg | *-dbgsym) ;;
        *) runtime_owners+=("$candidate") ;;
      esac
    done
    if [ "${#owners[@]}" -eq 0 ]; then
      fail "no dpkg package owns bundled library ${lib}: $(bundled_paths "$lib")"
    fi
    if [ "${#runtime_owners[@]}" -gt 1 ]; then
      echo "::error::ambiguous package owner for bundled library ${lib}: $(bundled_paths "$lib")" >&2
      fail "candidate packages: ${owners[*]}"
    fi
    if [ "${#runtime_owners[@]}" -eq 1 ]; then
      package="${runtime_owners[0]}"
    elif [ "${#owners[@]}" -eq 1 ]; then
      # Only a -dev/-dbg package owns the path. Keep its notice: the single
      # candidate is unambiguous even though it is not the preferred package.
      package="${owners[0]}"
    else
      echo "::error::ambiguous package owner for bundled library ${lib}: $(bundled_paths "$lib")" >&2
      fail "candidate packages: ${owners[*]}"
    fi

    copyright="${sysroot}/usr/share/doc/${package}/copyright"
    if [ ! -f "$copyright" ]; then
      echo "::warning::no copyright notice for ${package} (${lib})" >&2
      missing_notice=1
      continue
    fi
    install -Dm644 "$copyright" "${notice_dir}/${package}.copyright"

    # Debian/Ubuntu copyright files often point at
    # /usr/share/common-licenses/... instead of inlining GPL/LGPL text. Reject
    # path traversal, then copy every referenced file into the AppImage; a
    # dangling reference would ship an incomplete notice.
    if grep -qE '/usr/share/common-licenses/[^[:space:]]*\.\.' "$copyright"; then
      fail "unsafe '..' in a common-licence reference in ${package}.copyright (${lib})"
    fi
    while IFS= read -r reference; do
      [ -n "$reference" ] || continue
      source_file="${sysroot}${reference}"
      if [ ! -f "$source_file" ]; then
        fail "${package}.copyright (${lib}) references missing ${reference}"
      fi
      install -Dm644 "$source_file" "${common_license_dir}/${reference##*/}"
    done < <(grep -oE "$common_ref_re" "$copyright" | sort -u || true)

    printf '%s\t%s\n' "$lib" "$package" >> "${notice_dir}/SOURCES.txt"
  done < "$libs_file"

  if [ "$missing_notice" -ne 0 ]; then
    fail "a bundled system library is missing its copyright notice"
  fi
  echo "package-notices: installed system-library notices in ${notice_dir}"
  exit 0
fi

if [ "$mode" != "validate" ]; then
  usage
fi

[ -d "$notice_dir" ] || fail "AppImage is missing usr/share/doc/vector-field-explorer/system-libraries"
[ -f "${notice_dir}/SOURCES.txt" ] || fail "AppImage is missing system-libraries/SOURCES.txt"
[ -d "$common_license_dir" ] || fail "AppImage is missing system-libraries/common-licenses"

notice="$(find "$notice_dir" -maxdepth 1 -type f -name '*.copyright' -print -quit)"
# A linuxdeploy run that bundles no shared libraries produces an empty
# system-libraries directory (SOURCES.txt header only). That is a valid
# outcome, so the copyright check applies only when libraries were bundled.
bundled_libs="$(find "${target}/usr/lib" -maxdepth 2 \( -type f -o -type l \) \
  -name '*.so*' -print -quit 2>/dev/null || true)"
if [ -n "$bundled_libs" ]; then
  [ -n "$notice" ] || fail "AppImage has no bundled system-library copyright notices"
fi

# Every /usr/share/common-licenses/... reference in a shipped copyright must
# resolve inside the AppImage, not just on the runner.
missing=0
while IFS= read -r copyright; do
  if grep -qE '/usr/share/common-licenses/[^[:space:]]*\.\.' "$copyright"; then
    echo "::error::unsafe '..' in a common-licence reference in ${copyright##*/}" >&2
    missing=1
    continue
  fi
  while IFS= read -r reference; do
    [ -n "$reference" ] || continue
    relative="${reference#/usr/share/common-licenses/}"
    shipped="${common_license_dir}/${relative}"
    if [ ! -f "$shipped" ]; then
      echo "::error::AppImage is missing ${reference} referenced by ${copyright##*/}" >&2
      missing=1
    fi
  done < <(grep -oE "$common_ref_re" "$copyright" | sort -u || true)
done < <(find "$notice_dir" -maxdepth 1 -type f -name '*.copyright' -print)

if [ "$missing" -ne 0 ]; then
  fail "AppImage has unresolved /usr/share/common-licenses references"
fi

echo "package-notices: system-library notices are complete"

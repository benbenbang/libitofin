#!/usr/bin/env sh
# Set the workspace version in Cargo.toml AND the workspace-member entries in
# Cargo.lock. Invoked by semantic-release (@semantic-release/exec prepareCmd)
# so the crate version tracks the release; both files are committed by
# @semantic-release/git (both must be in its assets list).
#
#   sh scripts/set-version.sh 1.5.1
#
# Uses awk only (no cargo / GNU-sed extensions) so it runs in the minimal
# semantic-release container image, which has no cargo. The Cargo.lock pass
# rewrites the version of every source-less [[package]] block (the local
# workspace crates - registry deps carry a source line); the lockfile-format
# "version = N" line sits above the first [[package]] and is left untouched.
set -eu

version="${1:?usage: set-version.sh <version>}"

tmp="$(mktemp)"
awk -v v="$version" '
  /^\[/            { in_pkg = ($0 == "[workspace.package]") }
  in_pkg && !done && /^version[[:space:]]*=/ {
      print "version = \"" v "\""
      done = 1
      next
  }
  { print }
' Cargo.toml > "$tmp"
mv "$tmp" Cargo.toml

tmp2="$(mktemp)"
awk -v v="$version" '
  function flush(   i) {
      for (i = 1; i <= n; i++) {
          if (!has_source && buf[i] ~ /^version[[:space:]]*=/)
              buf[i] = "version = \"" v "\""
          print buf[i]
      }
      n = 0; has_source = 0
  }
  /^\[/ { flush(); buf[++n] = $0; next }
  {
      if (n > 0) {
          if ($0 ~ /^source[[:space:]]*=/) has_source = 1
          buf[++n] = $0
      } else {
          print
      }
  }
  END { flush() }
' Cargo.lock > "$tmp2"
mv "$tmp2" Cargo.lock

echo "set [workspace.package] version = $version in Cargo.toml and Cargo.lock"

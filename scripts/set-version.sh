#!/usr/bin/env sh
# Set the [package] version in Cargo.toml. Invoked by semantic-release
# (@semantic-release/exec prepareCmd) so the crate version tracks the release.
#
#   sh scripts/set-version.sh 1.5.1
#
# Uses awk only (no cargo / GNU-sed extensions) so it runs in the minimal
# semantic-release container image.
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
echo "set Cargo.toml [package] version = $version"

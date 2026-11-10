#!/usr/bin/env bash
# Licensed to the Apache Software Foundation (ASF) under one
# or more contributor license agreements.  See the NOTICE file
# distributed with this work for additional information
# regarding copyright ownership.  The ASF licenses this file
# to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance
# with the License.  You may obtain a copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

# Script has been modified by
# - @benbenbang
#

set -u  # Ensure the script fails on undefined variables

export REMEMBER_LAST_ANSWER="true"

. "$( dirname "${BASH_SOURCE[0]}" )/init.sh"

function get_default_branch() {
    local default_branch=""

    if [[ -z "$default_branch" ]]; then
        if git show-ref --verify --quiet refs/remotes/origin/main; then
            default_branch="main"
        elif git show-ref --verify --quiet refs/remotes/origin/master; then
            default_branch="master"
        else
            default_branch="main"  # fallback
        fi
    fi
    echo "$default_branch"
}

function hadolint_runner(){
    hadolint=$(which hadolint)

    if [[ -z "$hadolint" ]]; then
        docker run \
            -v "$(pwd):/root" \
            -w /root \
            --rm \
            hadolint/hadolint /bin/hadolint --config ./scripts/hadolint.yml "$@" || return 1
    else
        $hadolint --config ./scripts/hadolint.yml "$@" || return 1
    fi
}

# Emits one candidate path per line: the filenames pre-commit passes when the
# hook fires, or the diff against the default branch on a bare manual run.
function collect_candidates() {
    if [[ $# -gt 0 ]]; then
        printf '%s\n' "$@"
        return 0
    fi

    local default_branch
    default_branch=$(get_default_branch)

    git diff --diff-filter=d --name-only "${default_branch}" || true
}

function lint_wrapper() {
    local files=()
    local path

    if [[ "${1:-}" == "all" ]]; then
        echo
        echo -e "${GREY}Running docker lint for all Dockerfiles${NC}"
        if ! hadolint_runner Dockerfile*; then
            return 1
        fi
        echo -e "✅ ${GREEN}Dockerlint completed${NC}"
        echo
        return 0
    fi

    while IFS= read -r path; do
        [[ "$path" =~ Dockerfile ]] || continue
        [[ -f "$path" ]] || continue
        files+=("$path")
    done < <(collect_candidates "$@")

    if [[ ${#files[@]} -eq 0 ]]; then
        echo -e "${GREY}No Dockerfiles changed${NC}"
        return 0
    fi

    echo
    echo -e "${GREY}Running docker lint for ${files[*]}${NC}"
    for path in "${files[@]}"; do
        echo "Linting $path"

        if ! hadolint_runner "$path"; then
            return 1
        fi
    done
    echo -e "✅ ${GREEN}Dockerlint completed${NC}"
    echo
}

function run_docker_lint(){
    if ! lint_wrapper "$@" ; then
        echo -e "❌ ${RED}Dockerlint failed${NC}"
        exit 1
    fi
}

run_docker_lint "$@"

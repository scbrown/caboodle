#!/bin/sh
set -eu

caboodle_bin=${1:-caboodle}

"$caboodle_bin" --version >/dev/null
for command in init plan apply verify install project-settings verify-questions render-observability validate-observability; do
    "$caboodle_bin" "$command" --help >/dev/null
done

"$caboodle_bin" init --help | grep -q -- '--guided'
"$caboodle_bin" plan --help | grep -q -- '--profile'
"$caboodle_bin" plan --help | grep -q -- '--intent'
"$caboodle_bin" apply --help | grep -q -- '--skip-install'
"$caboodle_bin" verify --help | grep -q -- '--state'
"$caboodle_bin" verify --help | grep -q -- '--creel-doctor'
"$caboodle_bin" verify --help | grep -q -- '--creel-admission'
"$caboodle_bin" install --help | grep -q -- '--creel-doctor'
"$caboodle_bin" install --help | grep -q -- '--creel-admission'
"$caboodle_bin" verify-questions --help | grep -q -- '--plan'

printf '%s\n' 'caboodle skill surface: verified'

#!/bin/bash
# QML tests and lint for the plugin, as CI runs them.
#
# qmltestrunner passes tests whose QML logs a ReferenceError or binds
# undefined to a property: such warnings from the plugin's own files fail
# the run here. qmllint skips what the qs.* stubs cannot type (unqualified
# lookups, missing properties, Quickshell signal types) and Panel.qml, which
# needs omarchy-shell's own modules.
set -uo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
runner=${QMLTESTRUNNER:-$(command -v qmltestrunner || echo /usr/lib/qt6/bin/qmltestrunner)}
lint=${QMLLINT:-$(command -v qmllint || echo /usr/lib/qt6/bin/qmllint)}
log=$(mktemp)
trap 'rm -f "$log"' EXIT
fail=0

mkdir -p target/qml-shots
# Always offscreen: a desktop session's wayland changes geometry and timing.
QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen \
  "$runner" -input tests/qml -import tests/qml/stubs >"$log" 2>&1
status=$?
grep -E "^(FAIL|Totals)" "$log"
[[ $status == 0 ]] || { echo "QML tests failed"; fail=1; }

warnings=$(grep -E "^QWARN" "$log" | grep -E "/plugin/|overrides a member" | sort -u)
if [[ -n $warnings ]]; then
  echo "QML warnings from the plugin:"
  echo "$warnings"
  fail=1
fi

mapfile -t files < <(ls plugin/*.qml | grep -v '/Panel.qml$')
if ! "$lint" -I tests/qml/stubs \
    --unqualified disable --missing-property disable --compiler disable \
    --signal-handler-parameters info --unused-imports warning --max-warnings 0 "${files[@]}"; then
  echo "qmllint found problems"
  fail=1
fi

exit $fail

#!/bin/bash
# QML tests and lint for the plugin, as CI runs them.
#
# qmltestrunner passes tests whose QML logs a ReferenceError or binds
# undefined to a property: such warnings from the plugin's own files fail
# the run here. qmllint skips what the qs.* stubs cannot type (unqualified
# lookups, missing properties, Quickshell signal types) and Panel.qml and
# ModalPanel.qml, which need omarchy-shell's own modules. Qt 6.12's style
# advisories are skipped; unresolved types and runtime warnings fail.
set -uo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
runner=${QMLTESTRUNNER:-$(command -v qmltestrunner || echo /usr/lib/qt6/bin/qmltestrunner)}
lint=${QMLLINT:-$(command -v qmllint || echo /usr/lib/qt6/bin/qmllint)}
log=$(mktemp)
lint_log=$(mktemp)
trap 'rm -f "$log" "$lint_log"' EXIT
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

# Quickshell stops synthesizing types for a directory that has a qmldir, so
# every component must be listed there, not only the singletons.
missing=$(for f in plugin/*.qml; do
  grep -qE "^(singleton )?$(basename "$f" .qml) [0-9.]+ $(basename "$f")$" plugin/qmldir || basename "$f"
done)
if [[ -n $missing ]]; then
  echo "plugin/qmldir does not list:"
  echo "$missing"
  fail=1
fi

mapfile -t files < <(ls plugin/*.qml | grep -vE '/(Modal)?Panel.qml$')
if ! "$lint" -i plugin/qmldir -I tests/qml/stubs \
    --unqualified disable --missing-property disable --compiler disable \
    --renamed-type disable --id-shadows-member disable --block-scope-var-declaration disable \
    --signal-handler-parameters info --unused-imports warning --max-warnings 1 "${files[@]}" >"$lint_log" 2>&1; then
  cat "$lint_log"
  echo "qmllint found problems"
  fail=1
elif grep -qE '^(Warning|Error):' "$lint_log"; then
  cat "$lint_log"
  echo "qmllint found problems"
  fail=1
fi

exit $fail

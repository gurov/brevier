#!/usr/bin/env bash
# Регрессия проверки `--check`: отчёт против эталона.
#
#   corpus/check.sh            # прогнать все фикстуры и сравнить с эталонами
#   corpus/check.sh --approve  # записать эталоны заново (после осознанной правки)
#
# По фикстуре corpus/check/<name>.html — эталон corpus/check/expected/<name>.md.
# Проверка идёт через --stdin, поэтому сети нет и отчёт детерминирован: та же
# страница даёт те же байты, ровно как corpus/expected/ для самого чтения.
# Каждая фикстура вскрывает свою группу находок — по одной на неисправность,
# как просит роадмап.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dir="$root/corpus/check"
approve=0

while [[ $# -gt 0 ]]; do
    case $1 in
        --approve) approve=1; shift ;;
        -h|--help) sed -n '2,9p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "check.sh: неизвестный аргумент $1" >&2; exit 1 ;;
    esac
done

bin="$root/target/debug/brevier"
[[ -x "$bin" ]] || cargo build -q --manifest-path "$root/Cargo.toml"

mkdir -p "$dir/expected"
fail=0

for html in "$dir"/*.html; do
    name=$(basename "$html" .html)
    url="https://check.example/$name"
    expected="$dir/expected/$name.md"

    # --check выходит с ненулём, когда балл ниже порога: для регрессии это норма,
    # мы сверяем текст отчёта, а не код возврата.
    got=$("$bin" --check --stdin "$url" < "$html" || true)

    if [[ $approve -eq 1 ]]; then
        printf '%s' "$got" > "$expected"
        echo "approve $name"
        continue
    fi

    if [[ ! -f "$expected" ]]; then
        echo "НЕТ ЭТАЛОНА $name — запусти corpus/check.sh --approve" >&2
        fail=1
        continue
    fi
    if diff -u "$expected" <(printf '%s' "$got") >/dev/null; then
        echo "ok   $name"
    else
        echo "ДИФФ $name:" >&2
        diff -u "$expected" <(printf '%s' "$got") >&2 || true
        fail=1
    fi
done

exit $fail

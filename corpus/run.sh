#!/usr/bin/env bash
# Прогон корпуса M0.
#
#   corpus/run.sh --ua honest            # честный Brevier/0.1
#   corpus/run.sh --ua browser           # маскировка под браузер
#   corpus/run.sh --ua honest --jobs 8
#
# Кладёт markdown в corpus/out/<ua>/, отчёт в corpus/out/report-<ua>.tsv
# и заготовку вердиктов corpus/out/verdict-<ua>.tsv (колонка вердикта — `?`,
# проставляется руками по corpus/RUBRIC.md).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ua=honest
jobs=4
urls="$root/corpus/urls.txt"

# Один URL: вызывается через xargs, поэтому режим спрятан в первом аргументе.
if [[ "${1:-}" == "--one" ]]; then
    url=$2 outdir=$3 bin=$4 ua=$5
    slug=$(printf '%s' "$url" | sed -e 's|^https\?://||' -e 's|[^A-Za-z0-9._-]|-|g' | cut -c1-70)
    slug="$slug-$(printf '%s' "$url" | cksum | cut -d' ' -f1)"
    file="$outdir/$slug.md"
    errfile="$outdir/.$slug.err"
    if "$bin" --ua "$ua" "$url" >"$file" 2>"$errfile"; then code=0; else code=$?; fi
    # Сетевые обрывы случайны и шумят прямо в числе на гейте: одна повторная
    # попытка. Отказы доступа (403, сертификат) не повторяем — они устойчивы.
    if [[ $code -eq 2 ]]; then
        sleep 2
        if "$bin" --ua "$ua" "$url" >"$file" 2>"$errfile"; then code=0; else code=$?; fi
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$code" "$(wc -c <"$file")" "$url" "$slug.md" \
        "$(tr -d '\t\n' <"$errfile")"
    rm -f "$errfile"
    exit 0
fi

while [[ $# -gt 0 ]]; do
    case $1 in
        --ua) ua=$2; shift 2 ;;
        --jobs) jobs=$2; shift 2 ;;
        --urls) urls=$2; shift 2 ;;
        -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "run.sh: неизвестный аргумент $1" >&2; exit 1 ;;
    esac
done

cargo build --release --quiet --manifest-path "$root/Cargo.toml"
bin="$root/target/release/brevier"

outdir="$root/corpus/out/$ua"
report="$root/corpus/out/report-$ua.tsv"
verdict="$root/corpus/out/verdict-$ua.tsv"
mkdir -p "$outdir"

grep -vE '^\s*(#|$)' "$urls" \
    | xargs -P "$jobs" -I{} "${BASH_SOURCE[0]}" --one {} "$outdir" "$bin" "$ua" \
    | sort -t$'\t' -k3 > "$report.body"

{ printf 'exit\tbytes\turl\tfile\terror\n'; cat "$report.body"; } > "$report"
rm -f "$report.body"

# Вердикты — ручная работа, и потерять её нельзя: файл не перезаписываем,
# а дополняем. Уже проставленные `y` и `n` остаются, новые страницы приходят
# с `?`. (Однажды перезаписали — разметку спасли только эталоны в git.)
previous=$(mktemp); trap 'rm -f "$previous"' EXIT
[[ -f "$verdict" ]] && tr -d '\r' < "$verdict" > "$previous"

{
    printf 'verdict\turl\tfile\n'
    awk -F'\t' -v prev="$previous" '
        BEGIN { while ((getline line < prev) > 0) { split(line, f, "\t"); mark[f[2]] = f[1] } }
        NR > 1 && $1 == 0 {
            v = ($3 in mark && mark[$3] != "?" && mark[$3] != "verdict") ? mark[$3] : "?"
            printf "%s\t%s\t%s\n", v, $3, $4
        }' "$report"
} > "$verdict"

total=$(($(wc -l <"$report") - 1))
ok=$(awk -F'\t' 'NR>1 && $1==0' "$report" | wc -l)
echo
echo "корпус: $total страниц, UA=$ua"
echo "  извлеклось автоматически: $ok"
awk -F'\t' 'NR>1 && $1!=0 { c[$1]++ } END {
    n[1] = "плохой url"; n[2] = "сеть"; n[3] = "http-статус"
    n[4] = "content-type"; n[5] = "пустое извлечение"; n[6] = "конвертация"
    for (code in c) printf "  %s: %s\n", (code in n ? n[code] : "код " code), c[code]
}' "$report"
echo
echo "дальше: разметить вердикты в $verdict по corpus/RUBRIC.md,"
echo "        затем corpus/score.sh --ua $ua"

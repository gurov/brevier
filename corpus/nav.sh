#!/usr/bin/env bash
# Навигационный замер M0: куда ведут ссылки из читаемых статей.
#
#   corpus/nav.sh --ua honest [--articles 10] [--links 10] [--jobs 4]
#
# Берёт статьи, размеченные как читаемые (`y` в verdict-<ua>.tsv), с каждой
# берёт первые N исходящих ссылок и пробует прочитать их так же, как это
# сделал бы пользователь. Результат — corpus/out/nav-<ua>.tsv и доля ссылок,
# приведших на извлекаемую страницу.
#
# Это верхняя оценка: скрипт видит «извлеклось / не извлеклось», а не
# «читаемо по рубрике». Итоговое число — после ручной проверки выборки.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ua=honest
articles=10
per_article=10
jobs=4
seed=brevier-m0

if [[ "${1:-}" == "--one" ]]; then
    src=$2 url=$3 outdir=$4 bin=$5 ua=$6
    slug=$(printf '%s' "$url" | sed -e 's|^https\?://||' -e 's|[^A-Za-z0-9._-]|-|g' | cut -c1-60)
    slug="$slug-$(printf '%s' "$url" | cksum | cut -d' ' -f1)"
    file="$outdir/$slug.md"
    if "$bin" --ua "$ua" "$url" >"$file" 2>/dev/null; then code=0; else code=$?; fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$src" "$url" "$code" "$(wc -c <"$file")" "$slug.md"
    exit 0
fi

while [[ $# -gt 0 ]]; do
    case $1 in
        --ua) ua=$2; shift 2 ;;
        --articles) articles=$2; shift 2 ;;
        --links) per_article=$2; shift 2 ;;
        --jobs) jobs=$2; shift 2 ;;
        --seed) seed=$2; shift 2 ;;
        -h|--help) sed -n '2,13p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "nav.sh: неизвестный аргумент $1" >&2; exit 1 ;;
    esac
done

cargo build --release --quiet --manifest-path "$root/Cargo.toml"
bin="$root/target/release/brevier"

verdict="$root/corpus/out/verdict-$ua.tsv"
report="$root/corpus/out/report-$ua.tsv"
outdir="$root/corpus/out/nav-$ua"
nav="$root/corpus/out/nav-$ua.tsv"
mkdir -p "$outdir"

if [[ -f "$verdict" ]] && tr -d '\r' < "$verdict" | awk -F'\t' 'NR>1 && $1=="y"' | grep -q .; then
    # редактор мог сохранить вердикты с CRLF — возвраты убираем
    sources=$(tr -d '\r' < "$verdict" | awk -F'\t' 'NR>1 && $1=="y" { print $2 }')
else
    echo "вердиктов ещё нет — беру всё, что извлеклось; число будет завышено" >&2
    sources=$(awk -F'\t' 'NR>1 && $1==0 { print $3 }' "$report")
fi

sources=$(printf '%s\n' "$sources" | shuf -n "$articles" --random-source=<(yes "$seed"))

pairs=$(while read -r src; do
    [[ -n "$src" ]] || continue
    "$bin" --ua "$ua" --links "$src" 2>/dev/null | grep . | head -n "$per_article" \
        | while read -r target; do printf '%s\t%s\n' "$src" "$target"; done
done <<<"$sources")

printf '%s\n' "$pairs" | grep . \
    | xargs -P "$jobs" -d'\n' -I{} bash -c '
        IFS=$'"'"'\t'"'"' read -r src url <<<"$1"
        "$2" --one "$src" "$url" "$3" "$4" "$5"
    ' _ {} "${BASH_SOURCE[0]}" "$outdir" "$bin" "$ua" \
    | sort > "$nav.body"

{ printf 'source\ttarget\texit\tbytes\tfile\n'; cat "$nav.body"; } > "$nav"
rm -f "$nav.body"

total=$(($(wc -l <"$nav") - 1))
ok=$(awk -F'\t' 'NR>1 && $3==0' "$nav" | wc -l)
short=$(awk -F'\t' 'NR>1 && $3==0 && $4<1000' "$nav" | wc -l)
echo
echo "навигация, UA=$ua: извлеклось $ok из $total ссылок"
[[ $total -gt 0 ]] && awk -v o="$ok" -v t="$total" 'BEGIN {
    p = 100 * o / t
    printf "  доля: %.0f%% — порог 50%% %s\n", p, (p >= 50 ? "пройден" : "НЕ пройден")
}'
echo "  из них подозрительно коротких (<1000 байт): $short"
awk -F'\t' 'NR>1 && $3!=0 { c[$3]++ } END {
    n[2] = "сеть"; n[3] = "http-статус"; n[4] = "content-type"; n[5] = "пустое извлечение"
    for (code in c) printf "  %s: %s\n", (code in n ? n[code] : "код " code), c[code]
}' "$nav"

#!/usr/bin/env bash
# Счёт по вердиктам и гейт M0.
#
#   corpus/score.sh --ua honest              # посчитать
#   corpus/score.sh --ua honest --approve    # + сложить читаемые в corpus/expected/
#
# Вердикты берутся из corpus/out/verdict-<ua>.tsv: y — читаемо, n — нет,
# ? — ещё не смотрел. Знаменатель — весь корпус, а не только то, что скачалось:
# читателю не легче от того, что страницу не отдал Cloudflare.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ua=honest
approve=0

while [[ $# -gt 0 ]]; do
    case $1 in
        --ua) ua=$2; shift 2 ;;
        --approve) approve=1; shift ;;
        -h|--help) sed -n '2,9p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "score.sh: неизвестный аргумент $1" >&2; exit 1 ;;
    esac
done

verdict="$root/corpus/out/verdict-$ua.tsv"
report="$root/corpus/out/report-$ua.tsv"
[[ -f "$verdict" ]] || { echo "нет $verdict — сначала corpus/run.sh --ua $ua" >&2; exit 1; }

# Вердикты правит человек в редакторе, а редактор бывает с CRLF: `\r` уезжает
# в последнюю колонку и ломает имя файла. Читаем через копию без возвратов.
clean=$(mktemp); trap 'rm -f "$clean"' EXIT
tr -d '\r' < "$verdict" > "$clean"
verdict=$clean

total=$(($(wc -l <"$report") - 1))
yes=$(awk -F'\t' 'NR>1 && $1=="y"' "$verdict" | wc -l)
no=$(awk -F'\t' 'NR>1 && $1=="n"' "$verdict" | wc -l)
todo=$(awk -F'\t' 'NR>1 && $1!="y" && $1!="n"' "$verdict" | wc -l)

echo "UA=$ua: читаемых $yes из $total"
echo "  нечитаемых по рубрике: $no"
echo "  не размечено:          $todo"
# Пока размечено не всё, вердикта по гейту не выносим: он бы врал в обе
# стороны. Показываем вилку — что будет, если весь остаток окажется читаемым
# и если весь окажется нет.
if [[ $todo -gt 0 ]]; then
    awk -v y="$yes" -v t="$total" -v d="$todo" 'BEGIN {
        printf "  вилка: от %.0f%% до %.0f%% — гейт M0 (70%%) решится после разметки\n",
            100 * y / t, 100 * (y + d) / t
    }'
elif [[ $total -gt 0 ]]; then
    awk -v y="$yes" -v t="$total" 'BEGIN {
        p = 100 * y / t
        printf "  доля: %.0f%% — гейт M0 (70%%) %s\n", p, (p >= 70 ? "пройден" : "НЕ пройден")
    }'
fi

if [[ $approve -eq 1 ]]; then
    mkdir -p "$root/corpus/expected"

    # Эталон — это признанный читаемым вывод. Упавшая страница читаемой быть
    # не может, и класть её пустой вывод в эталоны значит молча стереть
    # регрессионную базу: вердикт-то остался с прошлого прогона, когда
    # страница открывалась. Поэтому сверяемся с кодом возврата.
    declare -A code
    while IFS=$'\t' read -r c _ _ file _; do code["$file"]=$c; done < <(tail -n +2 "$report")

    n=0
    skipped=0
    while IFS=$'\t' read -r v _ file; do
        [[ "$v" == "y" ]] || continue
        out="$root/corpus/out/$ua/$file"
        if [[ "${code[$file]:-1}" != 0 || ! -s "$out" ]]; then
            skipped=$((skipped + 1))
            continue
        fi
        cp "$out" "$root/corpus/expected/$file"
        n=$((n + 1))
    done < <(tail -n +2 "$verdict")

    echo "в corpus/expected/ положено эталонов: $n"
    if [[ $skipped -gt 0 ]]; then
        echo "  пропущено, страница в этот раз не открылась: $skipped"
    fi
fi

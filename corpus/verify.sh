#!/usr/bin/env bash
# Гигиена корпуса: сервер отдаёт по ссылке ту страницу, на которую ссылка указывает?
#
#   corpus/verify.sh [--urls corpus/urls.txt] [--jobs 4]
#
# Правило (сформулировано до прогона): URL остаётся в корпусе, только если
# итоговый URL после редиректов и `<link rel=canonical>` указывают на ту же
# страницу, что и запрошенный адрес. Проверка не зависит ни от Brevier,
# ни от рубрики, ни от разметчика — только от того, что ответил сервер.
#
# Зачем. vc.ru, dtf, elementy, theatlantic и им подобные ищут материал
# по числовому id и молча игнорируют слаг: по выдуманному адресу приходит
# 200 и чужая статья. Экстрактор её честно вытащит, разметчик увидит опрятный
# текст и поставит `y` — а это «нет» по пункту 6 рубрики, страница не та.
# Такая подмена не пахнет, поймать её глазами нельзя, поэтому — скриптом.
#
# Скрипт не выбрасывает ссылки сам: он помечает подозрительные, решение
# за человеком. Ложные срабатывания бывают — сайт канонизирует ветку
# в git-хостинге, кодирует скобки в percent-encoding и так далее.
#
# Результат — corpus/out/verify.tsv. Статусы:
#   ok       запрошенный адрес, итоговый и канонический сошлись
#   подмена  редирект увёл на другой адрес
#   канон    адрес тот же, но сайт считает канонической другую страницу
#   отказ    http-статус 4xx/5xx — проверить нечем
#   сеть     не достучались (dns, tls, таймаут)
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
urls="$root/corpus/urls.txt"
jobs=4
ua='Brevier/0.1'

# Сравниваем адреса как страницы, а не как строки: схема, `www.`, хвостовой
# слэш, якорь, регистр и percent-encoding различий не создают. Последнее важно
# для кириллицы: браузер отдаёт `/wiki/%D0%A2%D0%B5%D0%BE...`, а сайт кладёт
# в `canonical` те же буквы как есть — без декодирования это ложная тревога
# на каждой русской странице википедии.
norm() {
    local s=$1
    s=${s//\\/%5c}
    s=$(printf '%b' "${s//%/\\x}" 2>/dev/null || printf '%s' "$1")
    printf '%s' "$s" \
        | tr 'A-Z' 'a-z' \
        | sed -e 's|^https\?://||' -e 's|^www\.||' -e 's|#.*$||' -e 's|/$||'
}

if [[ "${1:-}" == "--one" ]]; then
    url=$2 ua=$3
    body=$(mktemp); trap 'rm -f "$body"' EXIT
    if ! out=$(curl -sSL -A "$ua" -m 30 -o "$body" \
                    -w '%{http_code}\t%{url_effective}' "$url" 2>/dev/null); then
        printf 'сеть\t\t%s\t\t\n' "$url"
        exit 0
    fi
    code=${out%%$'\t'*}
    final=${out#*$'\t'}

    # Канонический адрес: сперва rel=canonical, если его нет — og:url.
    canon=$(tr '\n' ' ' <"$body" \
        | grep -oiE '<link[^>]+rel=["'"'"']?canonical["'"'"']?[^>]*>' | head -1 \
        | grep -oiE 'href=["'"'"']?[^"'"'"' >]+' | sed -e 's/^[Hh][Rr][Ee][Ff]=//' -e 's/^["'"'"']//' || true)
    [[ -n "$canon" ]] || canon=$(tr '\n' ' ' <"$body" \
        | grep -oiE '<meta[^>]+property=["'"'"']?og:url["'"'"']?[^>]*>' | head -1 \
        | grep -oiE 'content=["'"'"']?[^"'"'"' >]+' | sed -e 's/^[Cc][Oo][Nn][Tt][Ee][Nn][Tt]=//' -e 's/^["'"'"']//' || true)

    if [[ $code -ge 400 ]]; then
        status=отказ
    elif [[ "$(norm "$final")" != "$(norm "$url")" ]]; then
        status=подмена
    elif [[ -n "$canon" && "$(norm "$canon")" != "$(norm "$url")" ]]; then
        status=канон
    else
        status=ok
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$status" "$code" "$url" "$final" "$canon"
    exit 0
fi

while [[ $# -gt 0 ]]; do
    case $1 in
        --urls) urls=$2; shift 2 ;;
        --jobs) jobs=$2; shift 2 ;;
        -h|--help) sed -n '2,26p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "verify.sh: неизвестный аргумент $1" >&2; exit 1 ;;
    esac
done

report="$root/corpus/out/verify.tsv"
mkdir -p "$root/corpus/out"

grep -vE '^\s*(#|$)' "$urls" \
    | xargs -P "$jobs" -I{} "${BASH_SOURCE[0]}" --one {} "$ua" \
    | sort -t$'\t' -k3 > "$report.body"

{ printf 'status\thttp\turl\tfinal\tcanonical\n'; cat "$report.body"; } > "$report"
rm -f "$report.body"

total=$(($(wc -l <"$report") - 1))
echo
echo "проверено ссылок: $total"
awk -F'\t' 'NR>1 { c[$1]++ } END { for (s in c) printf "  %-8s %s\n", s, c[s] }' "$report"

# Главное число: сколько подмен сидит среди страниц, уже размеченных читаемыми.
# Каждая такая — минус один из числителя на гейте.
verdict="$root/corpus/out/verdict-honest.tsv"
if [[ -f "$verdict" ]]; then
    echo
    echo "среди размеченных читаемыми (verdict-honest.tsv):"
    tr -d '\r' <"$verdict" | awk -F'\t' -v rep="$report" '
        BEGIN {
            while ((getline line < rep) > 0) { split(line, f, "\t"); st[f[3]] = f[1]; fin[f[3]] = f[4] }
        }
        NR > 1 && $1 == "y" && ($2 in st) && st[$2] != "ok" && st[$2] != "отказ" && st[$2] != "сеть" {
            n++; printf "  %s\n    %s → %s\n", st[$2], $2, fin[$2]
        }
        END { printf "  итого под вопросом: %d\n", n + 0 }'
fi
echo
echo "дальше: глазами пройти подозрительные строки в $report,"
echo "        выбросить выдуманные ссылки из corpus/urls.txt и перемерить"

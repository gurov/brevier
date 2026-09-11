#!/usr/bin/env bash
# Замер гейта M2: доходит ли читатель до всей документации репозитория,
# ни разу не открыв github.com.
#
#   corpus/m2.sh [--repos corpus/repos.txt] [--visits 60] [--refresh]
#                [--listing]
#
# `--listing` — не замер продукта, а оценка: что дал бы третий пункт плана,
# листинг каталога по требованию. Ссылка на каталог без README сейчас
# упирается в 404; с листингом читатель увидел бы, что в нём лежит,
# и спустился бы дальше. Каталоги берутся из того же кэшированного дерева,
# поэтому оценка не стоит ни одного запроса к API. По умолчанию выключено:
# число на гейте меряет то, что есть, а не то, что будет.
#
# Что считается — записано до прогона, как велит M0.
#
# Знаменатель: все markdown-файлы репозитория, кроме служебных
# (`.github/`, `node_modules/`, `vendor/`, `third_party/`, `target/`).
# Это и есть «документация целиком» из формулировки гейта.
#
# Числитель: сколько из них достижимо из README обходом в ширину
# по ссылкам, которые разворачивает сам Brevier. Внешние ссылки
# и ссылки в исходники не считаются ни тем, ни другим — по ним читатель
# уходит из документации осознанно.
#
# КРИТЕРИЙ ГЕЙТА, записанный до прогона:
#   1. медианная достижимость по репозиториям не ниже 80%;
#   2. недостижимое не образует систематической дыры. Проверяемо:
#      недостижимое разносится по категориям, и листинг каталога помогает
#      только одной из них — «в каталоге без README». Если она собирает
#      больше половины, гейт одними ссылками не берётся. Остальные три
#      («имя из обвязки хостинга», «README подкаталога», «рядом с README,
#      но не связан») лечатся не листингом, а другими средствами.
# Второй пункт важнее первого: он и есть тот вопрос, ради которого
# замер затевался.
#
# Дерево берётся через API хостинга. Это инструмент замера, а не продукта:
# в самом Brevier дерева нет и не планируется (см. TODO, M2).
# Замер только по github: у gitlab дерево постранично, и перечислить его
# стоит сотен запросов — что само по себе часть ответа.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin="$root/target/release/brevier"
list="$root/corpus/repos.txt"
outdir="$root/corpus/out"
visits=60
ua=honest
refresh=нет
listing=нет

while [[ $# -gt 0 ]]; do
    case $1 in
        --repos) list=$2; shift 2 ;;
        --visits) visits=$2; shift 2 ;;
        --refresh) refresh=да; shift ;;
        --listing) listing=да; shift ;;
        --ua) ua=$2; shift 2 ;;
        -h|--help) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "m2.sh: неизвестный аргумент $1" >&2; exit 1 ;;
    esac
done

[[ -x "$bin" ]] || { echo "m2.sh: нет $bin, соберите cargo build --release" >&2; exit 1; }
mkdir -p "$outdir"
report="$outdir/m2.tsv"
missed="$outdir/m2-missed.tsv"
if [[ "$listing" == да ]]; then
    report="$outdir/m2-listing.tsv"
    missed="$outdir/m2-listing-missed.tsv"
fi
printf 'repo\tdocs\treached\tshare\tbroken\tcapped\n' >"$report"
printf 'repo\tpath\tчто это\n' >"$missed"

# Все markdown-файлы репозитория. Один запрос к API на репозиторий,
# и ответ кладётся в кэш: лимит github — 60 запросов в час, а рубрику
# приходится уточнять, и каждое уточнение не должно стоить прогона.
docs_of() {
    local owner=$1 name=$2
    local cache="$outdir/trees/$owner-$name.json"
    mkdir -p "$outdir/trees"
    if [[ ! -s "$cache" || "$refresh" == да ]]; then
        curl -sS -A "Brevier/0.1" \
            "https://api.github.com/repos/$owner/$name/git/trees/HEAD?recursive=1" >"$cache"
    fi
    tr '{' '\n' <"$cache" |
    sed -n 's/.*"path":"\([^"]*\)".*"type":"blob".*/\1/p' |
    grep -Ei '\.(md|markdown)$' |
    # Что не является документацией проекта. Правила по форме, а не по
    # именам репозиториев, — иначе замер гниёт быстрее, чем проекты
    # меняют раскладку файлов.
    #
    #   * любой сегмент, начинающийся с точки: .github, .agents, .codex,
    #     .factory — конфиги инструментов, а не текст для читателя;
    #   * тесты и фикстуры: у bat в tests/syntax-tests лежит 42 файла
    #     LICENSE.md, по одному на грамматику;
    #   * зависимости, положенные в дерево;
    #   * инструкции для ИИ-инструментов: их читает не человек.
    grep -Eiv '(^|/)\.[^/]+/' |
    grep -Eiv '(^|/)(tests?|testdata|fixtures?|__tests__|benchsuite|examples?/generated)/' |
    grep -Eiv '(^|/)(node_modules|vendor|third_party|target|dist|build)/' |
    grep -Eiv '(^|/)(AGENTS|CLAUDE|GEMINI|CONVENTIONS)\.md$' || true
}

while read -r entry; do
    [[ -z "$entry" || "$entry" == \#* ]] && continue
    repo_addr=$entry
    slug=${entry#gh:}
    owner=${slug%%/*}
    name=${slug#*/}

    mapfile -t docs < <(docs_of "$owner" "$name")
    total=${#docs[@]}
    if (( total == 0 )); then
        printf '%s\t0\t0\t-\t-\tнет дерева\n' "$entry" >>"$report"
        continue
    fi

    declare -A known=() seen=() ; queue=() ; broken=0 ; capped=нет
    for d in "${docs[@]}"; do known["$d"]=1; done

    # Точка входа — тот README, который в репозитории действительно есть.
    for candidate in README.md readme.md README.markdown Readme.md; do
        if [[ -n "${known[$candidate]:-}" ]]; then queue=("$candidate"); break; fi
    done
    if (( ${#queue[@]} == 0 )); then queue=("${docs[0]}"); fi

    # Точки входа в документацию: README генераторных проектов ссылается
    # на собранный сайт, а не на исходники, и обход из него никуда
    # не приводит. Brevier ищет их пробой известных путей — замер обязан
    # начинать оттуда же, откуда начнёт читатель.
    #
    # Точка входа ставится в очередь, даже если её самой в знаменателе нет:
    # `.github/CONTRIBUTING.md` у deno отсеян фильтром служебных каталогов,
    # но читатель через него проходит, и всё, на что он ссылается, читателю
    # доступно. Знаменатель при этом не трогаем — числитель считается только
    # по нему, поэтому лишний узел в обходе число не надувает.
    while read -r entry_path _; do
        if [[ -n "$entry_path" ]]; then
            queue+=("$entry_path")
        fi
    done < <("$bin" --ua "$ua" --docs "$repo_addr" 2>/dev/null || true)

    visited=0
    while (( ${#queue[@]} > 0 )); do
        path=${queue[0]}; queue=("${queue[@]:1}")
        [[ -n "${seen[$path]:-}" ]] && continue
        seen["$path"]=1
        visited=$(( visited + 1 ))
        if (( visited > visits )); then capped=да; break; fi

        url="https://github.com/$owner/$name/blob/HEAD/$path"
        if ! links=$("$bin" --ua "$ua" --links "$url" 2>/dev/null); then
            broken=$(( broken + 1 )); continue
        fi
        while read -r link; do
            # Ветку в ссылке автор README пишет какую хочет — обычно `master`
            # или `main`, а наш разворот ставит `HEAD`. Читателю всё равно:
            # разбор адреса узнаёт любую. Замеру — тем более.
            case "$link" in
                "https://github.com/$owner/$name/blob/"*)
                    rest=${link#"https://github.com/$owner/$name/blob/"} ;;
                "https://github.com/$owner/$name/tree/"*)
                    rest=${link#"https://github.com/$owner/$name/tree/"} ;;
                *) continue ;;
            esac
            next=${rest#*/}
            next=${next%%#*}
            next=${next%%\?*}
            if [[ -z "$next" ]]; then continue; fi
            # Ссылка на каталог означает README внутри него.
            if [[ -z "${known[$next]:-}" && -n "${known[$next/README.md]:-}" ]]; then
                next="$next/README.md"
            elif [[ -z "${known[$next]:-}" && "$listing" == да ]]; then
                # Каталога без README сейчас не видно вовсе. Листинг показал бы
                # и файлы в нём, и подкаталоги — а подкаталог это ещё один
                # листинг, поэтому спуск бесплатный и берётся весь поддерев.
                for inside in "${docs[@]}"; do
                    if [[ "$inside" == "$next/"* && -z "${seen[$inside]:-}" ]]; then
                        queue+=("$inside")
                    fi
                done
                continue
            fi
            if [[ -n "${known[$next]:-}" && -z "${seen[$next]:-}" ]]; then
                queue+=("$next")
            fi
        done <<<"$links"
    done

    reached=0
    for d in "${docs[@]}"; do
        if [[ -n "${seen[$d]:-}" ]]; then reached=$(( reached + 1 )); fi
    done
    share=$(awk -v r="$reached" -v t="$total" 'BEGIN{printf "%.0f%%", 100*r/t}')
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$entry" "$total" "$reached" "$share" "$broken" "$capped" >>"$report"

    # Чем недостижимое является. Это и есть проверка второго пункта
    # критерия: листинг каталога помогает только последней категории.
    for d in "${docs[@]}"; do
        if [[ -n "${seen[$d]:-}" ]]; then continue; fi
        base=${d##*/}
        dir=${d%/*}; if [[ "$dir" == "$d" ]]; then dir=""; fi

        kind="в каталоге без README"
        # Известные имена хостинг показывает сам, своей обвязкой вокруг
        # страницы, а не ссылкой из README. Листинг тут ни при чём.
        if [[ "$base" =~ ^(CONTRIBUTING|CODE_OF_CONDUCT|SECURITY|CHANGELOG|CHANGES|AUTHORS|MAINTAINERS|LICENSE|COPYING|SUPPORT|GOVERNANCE)(\.[a-z-]+)?\.(md|markdown)$ ]]; then
            kind="имя из обвязки хостинга"
        elif [[ "$base" =~ ^[Rr][Ee][Aa][Dd][Mm][Ee] ]]; then
            kind="README подкаталога"
        else
            for candidate in README.md readme.md; do
                probe=${dir:+$dir/}$candidate
                if [[ -n "${known[$probe]:-}" ]]; then kind="рядом с README, но не связан"; fi
            done
        fi
        printf '%s\t%s\t%s\n' "$entry" "$d" "$kind" >>"$missed"
    done
    unset known seen
done <"$list"

echo
column -t -s$'\t' "$report"

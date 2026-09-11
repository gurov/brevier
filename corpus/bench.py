"""Внешний замер: прогон по чужому размеченному набору страниц.

    corpus/bench.py --label brevier --from markdown <клон бенчмарка> -- \
        target/release/brevier --stdin '{url}'

Ключи идут до пути: всё, что после пути, уходит в запускаемую команду.

Набор — `scrapinghub/article-extraction-benchmark` (MIT): 181 сохранённая
страница, 126 хостов, эталон — плоский текст тела статьи. Скрипт кладёт
наш вывод в их формат (`output/<label>.json`), дальше считает их же
`evaluate.py`, которому зависимости не нужны вовсе.

Зачем он нужен, хотя корпус у нас свой: наши числа посчитаны своей
рубрикой и своими руками, а тут линейка чужая, и в их таблице уже стоит
`dom_smoothie` — наш собственный движок на настройках по умолчанию.
Значит разница с его строкой — это ровно наш код.

Две решённые мелочи, от которых зависит число.

**Подаём плоский текст, а не markdown.** Метрика токенизирует по `\\w+`
и считает четырёхграммы, поэтому знаки разметки ей безразличны, а вот
адрес ссылки — нет: `[текст](https://site.com/slug)` дал бы пять лишних
токенов на каждую ссылку. Ссылку сводим к её тексту, адреса выбрасываем.

**Подписи картинок не берём ни там ни там.** В markdown у нас есть `alt`,
в извлечённом HTML он лежит атрибутом, и текстом его не видно. Взять его
в одном режиме и не взять в другом значило бы мерить разницу режимов,
а не разницу правил.
"""
import argparse, gzip, json, pathlib, re, subprocess, sys
from html.parser import HTMLParser


class Text(HTMLParser):
    """HTML → видимый текст. Своим разбором, без зависимостей: метрике
    нужны только слова, а `beautifulsoup` ради этого тянуть незачем."""

    SKIP = {"script", "style", "noscript", "template"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts, self.skipping = [], 0

    def handle_starttag(self, tag, attrs):
        if tag in self.SKIP:
            self.skipping += 1

    def handle_endtag(self, tag):
        if tag in self.SKIP and self.skipping:
            self.skipping -= 1

    def handle_data(self, data):
        if not self.skipping:
            self.parts.append(data)

    def text(self):
        # Пробел на каждом стыке тегов: иначе соседние элементы склеиваются
        # в одно слово, и четырёхграммы вокруг стыка ломаются на ровном месте.
        return " ".join(self.parts)


def from_html(source):
    parser = Text()
    parser.feed(source)
    return parser.text()


LINK = re.compile(r"!\[[^\]]*\]\([^)]*\)|\[([^\]]*)\]\([^)]*\)")
URL = re.compile(r"<?https?://[^\s)>\]]+>?")
FENCE = re.compile(r"^\s*```.*$", re.M)


def from_markdown(source):
    source = LINK.sub(lambda m: m.group(1) or "", source)
    source = URL.sub(" ", source)
    return FENCE.sub(" ", source)


READERS = {"html": from_html, "markdown": from_markdown}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bench", type=pathlib.Path, help="клон бенчмарка")
    ap.add_argument("--label", required=True, help="имя строки в их таблице")
    ap.add_argument("--from", dest="kind", required=True, choices=sorted(READERS))
    ap.add_argument("--version", default="", help="версия, как её печатать")
    ap.add_argument("cmd", nargs=argparse.REMAINDER, help="-- команда, {url} подставим")
    args = ap.parse_args()

    cmd = args.cmd[1:] if args.cmd[:1] == ["--"] else args.cmd
    if not cmd:
        sys.exit("нечего запускать: команда идёт после --")

    truth = json.loads((args.bench / "ground-truth.json").read_text("utf8"))
    read = READERS[args.kind]
    output, failed = {}, 0

    for n, (item, fields) in enumerate(sorted(truth.items()), 1):
        page = args.bench / "html" / f"{item}.html.gz"
        with gzip.open(page, "rt", encoding="utf8") as f:
            source = f.read()
        line = [part.replace("{url}", fields.get("url") or "") for part in cmd]
        done = subprocess.run(line, input=source, text=True, capture_output=True)
        if done.returncode != 0:
            # Отказ — это пустой ответ, а не пропуск строки: страница,
            # на которой мы ничего не показали, должна попасть в число.
            failed += 1
            body = ""
        else:
            body = read(done.stdout)
        output[item] = {"articleBody": body}
        print(f"\r{n}/{len(truth)}", end="", file=sys.stderr, flush=True)

    print(f"\r{len(truth)} страниц, отказов {failed}", file=sys.stderr)
    path = args.bench / "output" / f"{args.label}.json"
    path.write_text(
        json.dumps({"version": args.version, "output": output}, ensure_ascii=False),
        encoding="utf8",
    )
    print(path)


if __name__ == "__main__":
    main()

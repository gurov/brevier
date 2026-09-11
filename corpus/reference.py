"""Пособие для разметки корпуса: эталонный текст страницы против нашего вывода.

    corpus/reference.py fetch  <url>...   # DOM после JS → corpus/out/reference/
    corpus/reference.py digest <url>...   # выжимка сравнения по каждому адресу

Вердикт по рубрике ставит человек. Скрипт только показывает, что с чем
сравнивать: сколько абзацев статьи доехало, целы ли первый и последний,
нет ли у нас обвязки — подписки, «читайте ещё», комментариев.

Эталон берётся из headless chrome, потому что иначе сравнивать не с чем:
на JS-страницах текст появляется после разбора, и «что видит читатель
в браузере» — это именно отрисованный DOM. В продукт chrome не едет
никогда: здесь он инструмент мастерской, как `curl` в остальных скриптах
корпуса.

Цена записана честно: это единственный питон в проекте, и он тянет
`beautifulsoup4`. Разбирать чужой HTML регэкспом ради разметки — хуже.
"""
import html, os, re, shutil, subprocess, sys, tempfile, unicodedata
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Прогоны живут в corpus/out/ и в git не попадают — эталоны chrome тоже.
REF = os.path.join(ROOT, "corpus", "out", "reference")
os.makedirs(REF, exist_ok=True)

from bs4 import BeautifulSoup


def slug_of(url):
    for line in open(f"{ROOT}/corpus/out/report-honest.tsv", encoding="utf-8"):
        parts = line.rstrip("\n").split("\t")
        if len(parts) > 3 and parts[2].strip() == url.strip():
            return parts[3]
    return None


def dom_path(url):
    name = re.sub(r"[^A-Za-z0-9]", "-", url)[:80]
    return os.path.join(REF, name + ".html")


def fetch(url):
    """Профиль каждому запуску свой: один каталог chrome между процессами
    не делит, и на общем профиле параллельная качка молча отдаёт пустое —
    на сорока страницах из девяноста это и вышло."""
    out = dom_path(url)
    if os.path.exists(out) and os.path.getsize(out) > 2000:
        return out
    profile = tempfile.mkdtemp(prefix="brevier-chrome-")
    try:
        dom = subprocess.run(
            ["google-chrome", "--headless=new", "--disable-gpu", "--no-sandbox",
             f"--user-data-dir={profile}",
             "--virtual-time-budget=9000", "--dump-dom", url],
            capture_output=True, timeout=90).stdout.decode("utf-8", "replace")
    except Exception as e:
        dom = f"<!-- chrome не смог: {e} -->"
    finally:
        shutil.rmtree(profile, ignore_errors=True)
    open(out, "w", encoding="utf-8").write(dom)
    return out


def norm(text):
    text = unicodedata.normalize("NFKC", html.unescape(text))
    return re.sub(r"\s+", " ", text).strip()


def plain(text):
    """Только буквы и цифры в нижнем регистре: разметка, кавычки и тире
    у нас и у сайта разные, а слова — одни и те же."""
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)      # ссылка → её текст
    text = re.sub(r"[`*_#>|]", "", text)
    text = unicodedata.normalize("NFKC", html.unescape(text)).lower()
    return re.sub(r"[^0-9a-zа-яё ]+", " ", text.replace("ё", "е")).replace("  ", " ")


def squeeze(text):
    return re.sub(r"\s+", " ", plain(text)).strip()


# Обвязка страницы, которую эталон считать не должен: стена согласия,
# навигация, подвал, «читайте ещё», комментарии.
CHROME = re.compile(
    r"(?i)(cookie|consent|gdpr|privacy|qc-cmp|onetrust|truste|modal|dialog|popup|"
    r"nav|menu|header|footer|sidebar|related|recirc|newsletter|subscribe|promo|"
    r"comment|disqus|social|share|advert|sponsor)"
)


def reference_blocks(path):
    """Видимый текст статьи кусками. Берём `<article>`/`<main>`, если они есть:
    иначе у arstechnica эталоном становится диалог про куки на семьдесят
    абзацев, а не текст."""
    soup = BeautifulSoup(open(path, encoding="utf-8").read(), "html.parser")
    title = norm(soup.title.get_text()) if soup.title else ""
    for tag in soup(["script", "style", "noscript", "svg", "form", "iframe", "nav", "footer", "aside"]):
        tag.decompose()

    # Сначала выбираем статью, и только потом чистим обвязку внутри неё:
    # наоборот — и у arstechnica вместе с рекламной обёрткой уезжает сам текст.
    body = soup.find("article") or soup.find("main") or soup.find(attrs={"role": "main"}) or soup
    for tag in body.find_all(attrs={"class": CHROME}) + body.find_all(attrs={"id": CHROME}):
        tag.decompose()
    blocks = []
    for tag in body.find_all(["p", "li", "h1", "h2", "h3", "h4", "pre", "blockquote"]):
        text = norm(tag.get_text(" "))
        if text:
            blocks.append((tag.name, text))
    return title, blocks


JUNK = {
    "сырой html": r"<(div|span|p|a|img|table|section|figure)\b",
    "&nbsp;": r"&nbsp;|&amp;|&quot;",
    "base64": r"base64,",
    "куки/согласие": r"(?i)\b(cookie|consent|принима[ею]м куки|gdpr)\b",
    "подписка": r"(?i)(подпи(ши|са)|subscribe|newsletter|sign up for)",
    "читайте ещё": r"(?i)(читайте так|читайте ещ|related (articles|stories)|read more|you might also)",
    "комментарии": r"(?i)(оставить комментарий|comments? \(|add a comment|log in to comment)",
}


def digest(url):
    slug = slug_of(url)
    ours = ""
    if slug and os.path.exists(f"{ROOT}/corpus/out/honest/{slug}"):
        ours = open(f"{ROOT}/corpus/out/honest/{slug}", encoding="utf-8").read()
    title, blocks = reference_blocks(fetch(url))
    ours_flat = squeeze(ours)

    paras = [t for name, t in blocks if name == "p" and len(t) > 180]
    key = lambda p: squeeze(p)[:60]
    hit = [p for p in paras if key(p) and key(p) in ours_flat]
    lines = [l for l in ours.splitlines() if l.strip()]

    print(f"\n=== {url}")
    print(f"  наш вывод: {len(ours)} байт, {len(lines)} непустых строк, "
          f"заголовков {sum(1 for l in lines if l.startswith('#'))}, "
          f"ссылок {ours.count('](http')}, картинок {ours.count('![')}, "
          f"блоков кода {ours.count('```') // 2}, таблиц {sum(1 for l in lines if l.startswith('|'))}")
    print(f"  эталон (chrome): title «{title[:80]}»")
    share = f"{100 * len(hit) // max(len(paras), 1)}%"
    print(f"  абзацев >180 знаков: {len(paras)}, из них у нас {len(hit)} ({share})")
    if paras:
        print(f"  первый абзац: {'есть' if key(paras[0]) in ours_flat else 'НЕТ'} · "
              f"последний: {'есть' if key(paras[-1]) in ours_flat else 'НЕТ'}")
        lost = [p for p in paras if key(p) not in ours_flat]
        for p in lost[:6]:
            print(f"    потерян: {p[:110]}")
        if len(lost) > 6:
            print(f"    … и ещё {len(lost) - 6}")
    flags = [name for name, pattern in JUNK.items() if re.search(pattern, ours)]
    print(f"  признаки мусора у нас: {', '.join(flags) if flags else 'нет'}")
    print("  первые строки:")
    for l in lines[:6]:
        print(f"    | {l[:140]}")
    print("  последние строки:")
    for l in lines[-5:]:
        print(f"    | {l[:140]}")


if __name__ == "__main__":
    mode, urls = sys.argv[1], sys.argv[2:]
    if mode == "fetch":
        with ThreadPoolExecutor(max_workers=4) as pool:
            list(pool.map(fetch, urls))
        print(f"скачано: {len(urls)}")
    else:
        for url in urls:
            digest(url)

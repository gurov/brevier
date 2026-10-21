#!/usr/bin/env python3
"""Замороженный набор: тракт окна и телефона на сохранённых страницах.

    corpus/frozen.py snapshot [--refresh]   # HTML страниц корпуса — один раз
    corpus/frozen.py [run] [--approve] [--jobs N] [--grep S]

Прогон корпуса (`run.sh`) меряет, хорошо ли читается статья, и ходит в сеть:
страницы дрейфуют, вердикт ставит человек. Здесь другое — устойчивость.
Каждая страница проходит `examples/lay.rs`: markdown, модель страницы
с переносами, вехи, прогресс чтения, поиск, заметки — то, что cli не строит,
а окно и телефон строят на каждой странице. Ловится паника, зависание,
рост памяти, модель, которая не сходится сама с собой (код 70), и любое
отличие от одобренного вывода.

Страницы — с диска и не меняются: сохранённые HTML новостного замера
(`corpus/out/news/data`) и снимки страниц корпуса (`snapshot`, в
`corpus/out/frozen/html/`). Всё в `corpus/out/` и в git не едет: чужие
страницы в публичный репозиторий не кладём. Поэтому и прогон — перед
коммитом, у себя, а не в CI.

`--approve` кладёт вывод в одобренные (`approved/`); без него отличия —
повод посмотреть: `diff -u corpus/out/frozen/{approved,last}/<ключ>.md`.
Код возврата 1 — паника, зависание, несошедшаяся модель или неодобренное
отличие.
"""
import argparse, concurrent.futures as cf, filecmp, hashlib, json, os
import shutil, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "corpus", "out", "frozen")
NEWS = os.path.join(ROOT, "corpus", "out", "news", "data")
SNAPSHOTS = os.path.join(OUT, "html")
APPROVED = os.path.join(OUT, "approved")
LAST = os.path.join(OUT, "last")
LAY = os.path.join(ROOT, "target", "release", "examples", "lay")

# Страница укладывается за доли секунды; дольше этого — зависание.
TIMEOUT = 30
# Потолок памяти процесса: разбор одной страницы столько не просит никогда.
MEMORY = 4 << 30
# Медленнее этого — строкой в отчёте, даже если уложилась.
SLOW = 2.0
PANIC, BROKEN = 101, 70
UA = "Brevier/0.1"


def key_of(url):
    return hashlib.sha1(url.encode()).hexdigest()[:12]


def pages():
    """Адрес → HTML на диске: снимки корпуса и сохранённые новости."""
    found = {}
    index = os.path.join(SNAPSHOTS, "index.tsv")
    if os.path.exists(index):
        for line in open(index, encoding="utf-8"):
            url, name = line.rstrip("\n").split("\t")
            found[url] = os.path.join(SNAPSHOTS, name)
    for name in ("sample.jsonl", "sample2.jsonl"):
        path = os.path.join(NEWS, name)
        if not os.path.exists(path):
            continue
        for line in open(path, encoding="utf-8"):
            for article in json.loads(line).get("articles") or []:
                url = article.get("url")
                html = url and os.path.join(NEWS, "html", key_of(url) + ".html")
                if html and os.path.exists(html):
                    found.setdefault(url, html)
    home = os.path.join(NEWS, "home.tsv")
    if os.path.exists(home):
        for line in list(open(home, encoding="utf-8"))[1:]:
            url = line.rstrip("\n").split("\t")[-1]
            html = os.path.join(NEWS, "homehtml", key_of(url) + ".html")
            if os.path.exists(html):
                found.setdefault(url, html)
    return found


def snapshot(refresh):
    """HTML страниц корпуса — тем же запросом, что у Brevier. Снятое
    не перезаписывается: набор на то и замороженный."""
    os.makedirs(SNAPSHOTS, exist_ok=True)
    index = os.path.join(SNAPSHOTS, "index.tsv")
    known = {}
    if os.path.exists(index) and not refresh:
        for line in open(index, encoding="utf-8"):
            url, name = line.rstrip("\n").split("\t")
            known[url] = name
    urls = []
    for line in open(os.path.join(ROOT, "corpus", "urls.txt"), encoding="utf-8"):
        line = line.split("#")[0].strip()
        if line.startswith(("http://", "https://")) and line not in known:
            urls.append(line)

    def fetch(url):
        name = key_of(url) + ".html"
        path = os.path.join(SNAPSHOTS, name)
        r = subprocess.run(
            ["curl", "-sL", "--compressed", "--max-time", "30", "--max-filesize", str(8 << 20),
             "-A", UA, "-o", path, "-w", "%{http_code}\t%{content_type}", url],
            capture_output=True, text=True)
        code, _, kind = r.stdout.partition("\t")
        if r.returncode != 0 or code != "200" or "html" not in kind:
            if os.path.exists(path):
                os.remove(path)
            return url, None, f"{code} {kind or r.returncode}"
        return url, name, ""

    with cf.ThreadPoolExecutor(4) as pool:
        for url, name, why in pool.map(fetch, urls):
            if name:
                known[url] = name
            else:
                print(f"skipped {url}: {why}")
    with open(index, "w", encoding="utf-8") as out:
        for url, name in sorted(known.items()):
            out.write(f"{url}\t{name}\n")
    print(f"{len(known)} snapshots in {SNAPSHOTS}")


def lay(item):
    """Одна страница отдельным процессом: паника и зависание — её, а не
    прогона; память — пиком именно этого процесса (`wait4`)."""
    url, html = item
    key = key_of(url)
    md, model, err = (os.path.join(LAST, key + ext) for ext in (".md", ".json", ".err"))
    for path in (md, model):
        if os.path.exists(path):
            os.remove(path)
    start = time.time()
    with open(html, "rb") as page, open(err, "wb") as errors:
        # Потолок — `prlimit`, а не `preexec_fn`: тот в потоках небезопасен.
        # `prlimit` подменяет себя программой, и pid остаётся тем же.
        child = subprocess.Popen(["prlimit", f"--as={MEMORY}", LAY, url, md, model], stdin=page,
                                 stdout=subprocess.DEVNULL, stderr=errors)
        timed_out = False
        while True:
            pid, status, usage = os.wait4(child.pid, os.WNOHANG)
            if pid:
                break
            if time.time() - start > TIMEOUT:
                child.kill()
                pid, status, usage = os.wait4(child.pid, 0)
                timed_out = True
                break
            time.sleep(0.005)
        child.returncode = 0  # уже собран `wait4`: Popen не должен ждать его снова
    seconds = time.time() - start
    if timed_out:
        code = "timeout"
    elif os.WIFSIGNALED(status):
        code = f"signal {os.WTERMSIG(status)}"
    else:
        code = os.WEXITSTATUS(status)
    with open(err, encoding="utf-8", errors="replace") as errors:
        said = errors.read().strip().splitlines()
    return dict(key=key, url=url, code=code, seconds=seconds,
                memory=usage.ru_maxrss / 1024, said=said[0] if said else "")


def approved_codes():
    path = os.path.join(APPROVED, "codes.tsv")
    if not os.path.exists(path):
        return {}
    return dict(line.rstrip("\n").split("\t")[:2] for line in open(path, encoding="utf-8"))


def same(a, b):
    return os.path.exists(a) and os.path.exists(b) and filecmp.cmp(a, b, shallow=False)


def run(args):
    subprocess.run(["cargo", "build", "--quiet", "--release", "--example", "lay",
                    "--features", "typeset,images"], cwd=ROOT, check=True)
    found = pages()
    if args.grep:
        found = {url: html for url, html in found.items() if args.grep in url}
    if not found:
        sys.exit("no pages: run `corpus/frozen.py snapshot`, or the news measurement first")
    os.makedirs(LAST, exist_ok=True)
    with cf.ThreadPoolExecutor(args.jobs) as pool:
        results = list(pool.map(lay, sorted(found.items())))

    codes = approved_codes()
    failed, changed, new = [], [], []
    for r in results:
        if r["code"] in (PANIC, BROKEN) or not isinstance(r["code"], int):
            failed.append(r)
            continue
        if r["key"] not in codes:
            new.append(r)
            continue
        what = []
        if codes[r["key"]] != str(r["code"]):
            what.append(f"code {codes[r['key']]} → {r['code']}")
        if r["code"] == 0:
            for ext, name in ((".md", "markdown"), (".json", "model")):
                if not same(os.path.join(APPROVED, r["key"] + ext), os.path.join(LAST, r["key"] + ext)):
                    what.append(name)
        if what:
            r["what"] = ", ".join(what)
            changed.append(r)

    with open(os.path.join(OUT, "report.tsv"), "w", encoding="utf-8") as out:
        out.write("key\turl\tcode\tseconds\tmemory_mb\tchanged\tsaid\n")
        for r in results:
            out.write(f"{r['key']}\t{r['url']}\t{r['code']}\t{r['seconds']:.2f}\t"
                      f"{r['memory']:.0f}\t{r.get('what', '')}\t{r['said']}\n")

    tally = {}
    for r in results:
        tally[str(r["code"])] = tally.get(str(r["code"]), 0) + 1
    print(f"{len(results)} pages: " + ", ".join(f"code {c}: {n}" for c, n in sorted(tally.items())))
    for r in failed:
        print(f"FAILED {r['code']} {r['url']}\n    {r['said']}")
    slow = sorted((r for r in results if r["seconds"] > SLOW), key=lambda r: -r["seconds"])
    for r in slow[:10]:
        print(f"slow {r['seconds']:.1f} s {r['url']}")
    heavy = max(results, key=lambda r: r["memory"])
    print(f"most memory: {heavy['memory']:.0f} MB, {heavy['url']}")
    if changed:
        print(f"{len(changed)} changed against approved:")
        for r in changed[:20]:
            print(f"    {r['what']}: {r['url']}  ({r['key']})")
        if len(changed) > 20:
            print(f"    … the rest in {os.path.join(OUT, 'report.tsv')}")
    if new:
        print(f"{len(new)} not approved yet")

    if args.approve:
        os.makedirs(APPROVED, exist_ok=True)
        keep = {r["key"]: r for r in results if r not in failed}
        for key, r in keep.items():
            for ext in (".md", ".json"):
                src, dst = os.path.join(LAST, key + ext), os.path.join(APPROVED, key + ext)
                if r["code"] == 0:
                    shutil.copyfile(src, dst)
                elif os.path.exists(dst):
                    os.remove(dst)
            codes[key] = str(r["code"])
        with open(os.path.join(APPROVED, "codes.tsv"), "w", encoding="utf-8") as out:
            for key, code in sorted(codes.items()):
                out.write(f"{key}\t{code}\n")
        print(f"approved {len(keep)}")
        sys.exit(1 if failed else 0)
    sys.exit(1 if failed or changed or new else 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", nargs="?", default="run", choices=["run", "snapshot"])
    parser.add_argument("--approve", action="store_true")
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    parser.add_argument("--grep", default="")
    args = parser.parse_args()
    if args.command == "snapshot":
        snapshot(args.refresh)
    else:
        run(args)


if __name__ == "__main__":
    main()

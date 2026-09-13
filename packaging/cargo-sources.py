#!/usr/bin/env python3
"""Крейты из `Cargo.lock` — списком источников для flatpak-builder.

Flathub собирает без сети: всё, что попадёт в сборку, объявляется заранее
адресом и контрольной суммой. Для Rust это значит выложить каждый крейт
из `Cargo.lock` отдельным источником и подсунуть cargo вендорный каталог
вместо crates.io.

Чужой генератор (`flatpak-builder-tools/cargo`) тянет aiohttp и ходит
в сеть за суммами, которых в `Cargo.lock` и так довольно: `checksum`
у каждого пакета — это sha256 того самого `.crate`. Поэтому свой, на
стандартной библиотеке, и запускается он без сети:

    packaging/cargo-sources.py > packaging/cargo-sources.json

Перегонять — при каждом подъёме зависимостей, вместе с `Cargo.lock`.
"""

import json
import sys
import tomllib
from pathlib import Path

# Адрес, по которому crates.io раздаёт сам архив крейта. Вторая форма
# (`/api/v1/crates/<имя>/<версия>/download`) отвечает редиректом сюда же.
CRATE_URL = "https://static.crates.io/crates/{name}/{name}-{version}.crate"

# Куда кладётся вендорный каталог. Путь относительный: flatpak-builder
# считает его от каталога сборки модуля, а cargo — от своего рабочего
# каталога, и это одно и то же место.
VENDOR = "cargo/vendor"

CONFIG = """\
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "cargo/vendor"
"""


def sources(lock: dict) -> list[dict]:
    out = []
    for package in sorted(lock["package"], key=lambda p: (p["name"], p["version"])):
        # У пакета самого проекта нет ни источника, ни суммы: он приезжает
        # не из реестра, а из репозитория.
        checksum = package.get("checksum")
        source = package.get("source", "")
        if not checksum or not source.startswith("registry+"):
            if source:
                raise SystemExit(
                    f"{package['name']} {package['version']}: источник "
                    f"{source!r} этим генератором не поддержан"
                )
            continue

        name, version = package["name"], package["version"]
        dest = f"{VENDOR}/{name}-{version}"
        out.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": CRATE_URL.format(name=name, version=version),
                "sha256": checksum,
                "dest": dest,
            }
        )
        # Вендорный крейт cargo признаёт только с этим файлом. Суммы
        # по файлам он при этом не требует — хватает суммы архива,
        # а её уже проверил flatpak-builder.
        out.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )

    out.append(
        {
            "type": "inline",
            "contents": CONFIG,
            "dest": "cargo",
            "dest-filename": "config.toml",
        }
    )
    return out


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    json.dump(sources(lock), sys.stdout, indent=4)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()

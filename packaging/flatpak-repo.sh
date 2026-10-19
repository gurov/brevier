#!/bin/sh
# Свой репозиторий flatpak на сайте — обновления без Flathub (заявку туда
# мейнтейнер отложил 19 октября 2026). Собирается из бандла релиза,
# `brevier.flatpak` из Releases, при каждой выкладке сайта: Pages заменяет
# сайт целиком, и держать репозиторий между выкладками негде, а Releases
# хранят бандл всегда. В репозитории один коммит — последний: клиенту
# история не нужна, `flatpak update` сверяет коммит в сводке с установленным
# и берёт новый, если тот свежее.
#
#     packaging/flatpak-repo.sh <brevier.flatpak> <каталог>
#
# Подпись обязательна: `.flatpakref` и бандл приносят клиенту ключ, и коммит
# и сводку без подписи он не примет. Ключ — в $GNUPGHOME, отпечаток —
# в $FLATPAK_GPG_KEY_ID; публичная половина — `site/flatpak/brevier.gpg`.
# Нужны `flatpak` и `ostree`.
set -eu

bundle=$1
repo=$2
key=${FLATPAK_GPG_KEY_ID:?отпечаток ключа подписи}
home=${GNUPGHOME:?каталог GnuPG с ключом подписи}

rm -rf "$repo"
ostree init --mode=archive-z2 --repo="$repo"
flatpak build-import-bundle --no-update-summary \
    --gpg-sign="$key" --gpg-homedir="$home" "$repo" "$bundle"
# Дельта «с нуля» — установка одним файлом, а не сотней мелких запросов.
flatpak build-update-repo --generate-static-deltas \
    --gpg-sign="$key" --gpg-homedir="$home" \
    --title=Brevier --homepage=https://brevier.bizibah.com/ \
    --default-branch=master "$repo"

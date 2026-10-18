#!/usr/bin/env bash
# Restore the measured public corpora into durable user data.
#
# The old default, ~/Sites/temp-files/public-corpora, is a swept scratch area:
# libera-hd archived and deleted the corpora on 2026-09-29, which turned the
# fail-closed gate red on m5. ~/.local/share holds durable user data that no
# sweeper touches. A depth-1 pinned fetch carries no history and is the same
# method CI already used. This is one fetcher for both CI and local runs.
#
# Each missing corpus is fetched into a same-filesystem partial directory and
# renamed only after its detached HEAD has been checked. Existing trees are
# never moved: another measurement may be reading them.
#
# Exit status:
#   0  every selected declaration was restored or skipped without a baseline
#   1  an existing tree was wrong/unusable, or a fetch/verification failed
#  64  an argument, declaration, or selected id is malformed/unknown
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
CORPORA_FILE=${PUBLIC_CORPORA_FILE:-$ROOT/scripts/public-corpora.txt}
BASELINE_DIR=${PUBLIC_BASELINE_DIR:-$ROOT/scripts/public-baseline}
CORPORA_ROOT=${PUBLIC_CORPORA_ROOT:-$HOME/.local/share/itaruby/public-corpora}

usage() {
  printf 'usage: %s [--id <id>]...\n' "$0" >&2
}

selected=()
while (($#)); do
  case $1 in
    --id)
      (($# >= 2)) || { usage; exit 64; }
      selected+=("$2")
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf 'FAIL public corpus restore: unknown argument %q\n' "$1" >&2
      usage
      exit 64
      ;;
  esac
done

if [[ ! -f $CORPORA_FILE ]]; then
  printf 'FAIL public corpus restore: declaration file %s is missing\n' "$CORPORA_FILE" >&2
  exit 1
fi

ids=()
urls=()
shas=()
clones=()
while IFS= read -r raw || [[ -n $raw ]]; do
  [[ $raw =~ ^[[:space:]]*(#|$) ]] && continue
  read -r -a fields <<<"$raw"
  if (( ${#fields[@]} != 4 && ${#fields[@]} != 5 )); then
    printf 'FAIL public corpus restore: malformed repo line in %s\n' "$CORPORA_FILE" >&2
    exit 64
  fi
  id=${fields[0]}; url=${fields[1]}; sha=${fields[2]}; clone_dir=${fields[4]-$id}
  if [[ ! $id =~ ^[A-Za-z0-9._-]+$ || -z $url || ! $sha =~ ^[0-9a-f]{40}$ ||
        ! ${fields[3]} =~ ^[0-9]+([.][0-9]+)?(:[0-9]+([.][0-9]+)?)?$ ||
        -z $clone_dir || $clone_dir == /* || $clone_dir == . || $clone_dir == .. ]]; then
    printf 'FAIL public corpus restore: malformed repo line for %q in %s\n' "$id" "$CORPORA_FILE" >&2
    exit 64
  fi
  for prior in "${ids[@]}"; do
    [[ $prior != "$id" ]] || {
      printf 'FAIL public corpus restore: duplicate id %q in %s\n' "$id" "$CORPORA_FILE" >&2
      exit 64
    }
  done
  ids+=("$id")
  urls+=("$url")
  shas+=("$sha")
  clones+=("$clone_dir")
done <"$CORPORA_FILE"

for wanted in "${selected[@]}"; do
  found=0
  for id in "${ids[@]}"; do
    [[ $id == "$wanted" ]] && found=1 && break
  done
  if (( ! found )); then
    printf 'FAIL public corpus restore: unknown id %q\n' "$wanted" >&2
    exit 64
  fi
done

partial_dir=
cleanup_partial() {
  if [[ -n ${partial_dir:-} ]]; then
    rm -rf -- "$partial_dir"
    partial_dir=
  fi
}
trap cleanup_partial EXIT HUP INT TERM

failed=0
for ((i=0; i < ${#ids[@]}; i++)); do
  id=${ids[i]}
  if ((${#selected[@]})); then
    wanted=0
    for filter in "${selected[@]}"; do
      [[ $filter == "$id" ]] && wanted=1 && break
    done
    (( wanted )) || continue
  fi
  sha=${shas[i]}
  base="$BASELINE_DIR/$id.jsonl"
  dir="$CORPORA_ROOT/${clones[i]}"
  if [[ ! -s $base ]]; then
    printf 'skip %s (no baseline; never fetched)\n' "$id"
    continue
  fi

  if [[ -e $dir ]]; then
    # `--show-prefix` is empty only at a repository's top level: a plain
    # directory that happens to sit INSIDE another checkout (a fixture lab
    # under target/, a root inside a dotfiles repo) must not borrow that
    # checkout's HEAD.
    if ! prefix=$(git -C "$dir" rev-parse --show-prefix 2>/dev/null) || [[ -n $prefix ]]; then
      printf 'FAIL %s: %s is not a git repo; refusing to move a tree another measurement may read (re-pin deliberately: remove it or set PUBLIC_CORPORA_ROOT)\n' "$id" "$dir"
      failed=1
      continue
    fi
    head=$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)
    if [[ $head == "$sha" ]]; then
      printf 'ok %s already at %s\n' "$id" "${sha:0:7}"
    else
      printf 'FAIL %s: %s is at %s; refusing to move a tree another measurement may read (re-pin deliberately: remove it or set PUBLIC_CORPORA_ROOT)\n' "$id" "$dir" "${head:-<no HEAD>}"
      failed=1
    fi
    continue
  fi

  mkdir -p "$CORPORA_ROOT" "$(dirname "$dir")" || {
    printf 'FAIL %s: cannot create %s\n' "$id" "$CORPORA_ROOT"
    failed=1
    continue
  }
  partial_dir="$dir.partial.$$"
  if [[ -e $partial_dir ]]; then
    printf 'FAIL %s: partial path %s already exists; refusing to move a tree another measurement may read\n' "$id" "$partial_dir"
    # Not ours: the exit trap must not delete it.
    partial_dir=
    failed=1
    continue
  fi
  if ! git init -q "$partial_dir" ||
     ! git -C "$partial_dir" remote add origin "${urls[i]}" ||
     { ! git -C "$partial_dir" fetch -q --depth 1 origin "$sha" &&
       ! git -C "$partial_dir" fetch -q origin "$sha"; } ||
     ! git -C "$partial_dir" checkout -q --detach FETCH_HEAD; then
    printf 'FAIL %s: fetch failed for pinned sha %s\n' "$id" "$sha"
    cleanup_partial
    failed=1
    continue
  fi
  got=$(git -C "$partial_dir" rev-parse HEAD 2>/dev/null || true)
  if [[ $got != "$sha" ]]; then
    printf 'FAIL %s: fetched HEAD %s, expected %s\n' "$id" "${got:-<no HEAD>}" "$sha"
    cleanup_partial
    failed=1
    continue
  fi
  if ! mv "$partial_dir" "$dir"; then
    printf 'FAIL %s: could not install %s\n' "$id" "$dir"
    cleanup_partial
    failed=1
    continue
  fi
  partial_dir=
  printf 'fetched %s @ %s -> %s\n' "$id" "${sha:0:7}" "$dir"
done

exit "$failed"

#!/usr/bin/env bash
# Every `.unwrap()` in production source is one shape, and this proves it.
#
# The shape: `node.as_<kind>_node().unwrap()`, immediately inside a match arm
# that already discriminated on `Node::<Kind>Node { .. }`. ruby-prism models
# its AST as a variant enum whose payload is only reachable through a
# fallible downcast, so the match proves the downcast and the API forces the
# `unwrap`. There are dozens of these in the inference walk and they carry no
# risk.
#
# What this gate exists for is everything else. A reviewer greps `unwrap` and
# finds 60-odd hits; without a gate, the only way to know none of them is a
# real panic path is to read all of them, and the answer decays with the next
# commit. With a gate, "the only unwraps here are prism downcasts" is a
# checked property instead of a claim.
#
# Test code is exempt: a panic in a test is a failing test, which is the
# point. The exemption assumes each file's `#[cfg(test)]` module is its tail,
# and that assumption is itself checked below — a gate that can silently go
# blind is not a gate.
#
# Exit 0 clean, 1 with the offending lines named.
set -euo pipefail

cd "$(dirname "$0")/.."

offenders=0
blind=0

for f in crates/*/src/*.rs; do
  # First `#[cfg(test)]` line, or 0 when the file has no test module.
  cut_at=$(awk '/^#\[cfg\(test\)\]/{print NR; exit} END{if (!NR) print 0}' "$f")
  [ -z "$cut_at" ] && cut_at=0

  if [ "$cut_at" != "0" ]; then
    # Self-guard: production items after the test module would mean the
    # tail-exemption is silently skipping real code.
    stray=$(awk -v c="$cut_at" 'NR > c && /^(pub )?fn |^impl |^pub struct |^pub enum /{print NR}' "$f" | head -3)
    if [ -n "$stray" ]; then
      echo "BLIND $f — production items at line(s) $(echo "$stray" | tr '\n' ' ')appear after the #[cfg(test)] module at line $cut_at, so the test exemption is skipping real code. Move the test module to the tail, or teach this gate about the file."
      blind=1
    fi
    body=$(awk -v c="$cut_at" 'NR < c' "$f")
  else
    body=$(cat "$f")
  fi

  # Number the production region, keep `.unwrap()` lines, drop the sanctioned
  # prism downcast, drop comment lines (this file's own doc comments discuss
  # `.unwrap()` by name, and a gate that accuses prose is a gate people learn
  # to ignore), and report whatever survives.
  hits=$(printf '%s\n' "$body" \
    | grep -n '\.unwrap()' \
    | grep -v 'as_[a-z_]*_node()\.unwrap()' \
    | grep -vE '^[0-9]+:[[:space:]]*//' \
    || true)
  if [ -n "$hits" ]; then
    while IFS= read -r line; do
      echo "FAIL $f:$line"
      offenders=$((offenders + 1))
    done <<< "$hits"
  fi
done

if [ "$blind" != "0" ]; then
  echo "unwrap gate: FAIL — the gate cannot see part of the source it claims to cover"
  exit 1
fi

if [ "$offenders" != "0" ]; then
  echo "unwrap gate: FAIL — $offenders unwrap(s) in production source are not prism downcasts."
  echo "Either restructure so the value cannot be absent, or, if the invariant is real and local, say so with #[expect]/a named fallback instead of unwrap()."
  exit 1
fi

echo "unwrap gate: PASS — every unwrap() in production source is a prism downcast guarded by its match arm"

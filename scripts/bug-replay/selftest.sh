#!/usr/bin/env bash
# selftest.sh — the two-sided proof that bug-replay's classifier (judge.rb)
# judges at all, in every direction, against the synthetic fixture/ pair.
#
#   HIT        buggy.rb at parent (E0101 on `totall`), fixed.rb at fix
#              -> must classify HIT, with line_match (the diagnostic landed
#                 on the fixed line +-3)
#   NOISE      fixed.rb mutated to still be buggy (`refnd`, a DIFFERENT
#              undefined method) -> the fix revision still screams, a new
#              diagnostic the parent never had -> must classify NOISE
#   UNRELATED  fix side identical to the buggy parent -> the same diagnostic
#              survives unchanged -> must classify UNRELATED
#   MISS       clean on both sides -> silence everywhere -> must classify MISS
#
# Scratch lives under ~/Sites/temp-files (never /tmp). Exit 0 only when all
# four assertions hold.
set -uo pipefail

cd "$(dirname "$0")/../.."
ROOT=$PWD
HERE=$ROOT/scripts/bug-replay
FIXTURE=$HERE/fixture
ITA=$ROOT/target/release/ita

say() { printf '\n=== %s\n' "$1"; }
fail() { printf 'FAIL %s\n' "$1"; FAILED=1; }
pass() { printf 'PASS %s\n' "$1"; }
FAILED=0

SCRATCH=$HOME/Sites/temp-files/$(date +%Y%m%d-%H%M%S)-bug-replay-selftest
mkdir -p "$SCRATCH"
echo "scratch: $SCRATCH"

say 'build — cargo build --release --locked --target-dir ROOT/target (always, evidence rule)'
if cargo build --release --locked --target-dir "$ROOT/target" >"$SCRATCH/build.txt" 2>&1; then
  pass 'cargo build --release --locked --target-dir ROOT/target'
else
  printf 'FAIL cargo build --release (see %s/build.txt)\n' "$SCRATCH"
  exit 1
fi
pass "$ITA present (built from the checked build above)"

# Four case dirs: parent side vs fix side, one file each, same relative path.
mk_case() { # CASE SRC_PARENT SRC_FIX
  mkdir -p "$SCRATCH/$1/parent/app" "$SCRATCH/$1/fix/app"
  cp "$2" "$SCRATCH/$1/parent/app/widget.rb"
  cp "$3" "$SCRATCH/$1/fix/app/widget.rb"
}
mk_case hit        "$FIXTURE/buggy.rb"       "$FIXTURE/fixed.rb"
mk_case noise      "$FIXTURE/buggy.rb"       "$FIXTURE/still-buggy.rb"
mk_case unrelated  "$FIXTURE/buggy.rb"       "$FIXTURE/buggy.rb"
mk_case miss       "$FIXTURE/fixed.rb"       "$FIXTURE/fixed.rb"

# One synthetic record per case: the mined-pair shape replay.sh feeds judge.rb.
mk_record() { # CASE OLD_START NEW_START REMOVED added [SUBJECT]
  ruby -rjson -e '
    json = JSON.generate(
      "id" => "selftest-#{ARGV[0]}", "fix_sha" => "fix", "parent_sha" => "parent",
      "subject" => (ARGV[6] && !ARGV[6].empty?) ? ARGV[6] : "selftest #{ARGV[0]}",
      "files" => ["app/widget.rb"],
      "hunks" => [{"path" => "app/widget.rb", "old_start" => ARGV[1].to_i,
                   "new_start" => ARGV[2].to_i, "removed" => [ARGV[3]],
                   "added" => [ARGV[4]]}],
      "signal" => "rename")
    File.write(ARGV[5], json + "\n")' \
    "$1" "$2" "$3" "$4" "$5" "$SCRATCH/$1/record.json" "${6:-}"
}
mk_record hit       10 10 'invoice.totall' 'invoice.total'
mk_record noise     10 11 'invoice.totall' $'invoice.total\ninvoice.refnd'
mk_record unrelated 10 10 'invoice.totall' 'invoice.totall'
mkdir -p "$SCRATCH/hitq/parent/app" "$SCRATCH/hitq/fix/app"
mk_record hitq      3  3  'INVOICES << 1'  '' 'Fix ArgumentError when widget is dumped without invoices'
mk_record miss       5  5 '100' '100 # (comment only)'
# hitq fixture: the fix removes an E0104 line (undefined constant), but the
# subject claims ArgumentError — the label stays HIT and class_match must be
# false (the subject's failure family predicts E0102/E0103, got E0104).
mkdir -p "$SCRATCH/hitq/parent/app" "$SCRATCH/hitq/fix/app"
printf 'class Widget\n  def dump\n    INVOICES << 1\n  end\nend\n' >"$SCRATCH/hitq/parent/app/widget.rb"
printf 'class Widget\n  def dump\n  end\nend\n' >"$SCRATCH/hitq/fix/app/widget.rb"

say 'ita check on each side'
  for case in hit noise unrelated miss hitq; do
  for side in parent fix; do
    dir=$SCRATCH/$case/$side
    "$ITA" check --format=json "$dir" >"$SCRATCH/$case/$side.jsonl" 2>"$SCRATCH/$case/$side.err"
    printf '%-10s %-6s rc=%d diagnostics=%d\n' "$case" "$side" "$?" "$(wc -l <"$SCRATCH/$case/$side.jsonl" | tr -d ' ')"
  done
done

say 'judge — the classifier must answer the right label for every case'
expect() { # CASE EXPECTED [EXTRA_JQ-ish Ruby predicate]
  local case=$1 expected=$2 out label
  out=$(ruby "$HERE/judge.rb" --record="$SCRATCH/$case/record.json" \
    --parent-diags="$SCRATCH/$case/parent.jsonl" --parent-root="$SCRATCH/$case/parent" \
    --fix-diags="$SCRATCH/$case/fix.jsonl" --fix-root="$SCRATCH/$case/fix" --tool=ita)
  label=$(ruby -rjson -e 'puts JSON.parse(STDIN.read)["label"]' <<<"$out")
  if [[ $label == "$expected" ]]; then
    pass "$case -> $label"
  else
    fail "$case -> expected $expected, got $label"
    printf '%s\n' "$out"
  fi
  printf '%s\n' "$out" >"$SCRATCH/$case/judged.json"
}
expect hit       HIT
expect noise     NOISE
expect unrelated UNRELATED
expect miss      MISS
expect hitq      HIT

say 'line_match — the HIT diagnostic must land on the fixed line +-3'
lm=$(ruby -rjson -e 'puts JSON.parse(File.read(ARGV[0]))["line_match"]' "$SCRATCH/hit/judged.json")
if [[ $lm == true ]]; then pass 'hit line_match=true'; else fail "hit line_match expected true, got $lm"; fi

say 'class_match — a HIT whose silenced family contradicts the subject prints as HIT?'
hcm=$(ruby -rjson -e 'j = JSON.parse(File.read(ARGV[0])); puts "#{j["label"]}/#{j["class_match"]}"' "$SCRATCH/hitq/judged.json")
if [[ $hcm == HIT/false ]]; then
  pass 'hitq -> HIT with class_match=false (subject says ArgumentError, silenced E0104)'
else
  fail "hitq expected HIT/false, got $hcm"
fi

say 'judge srb format — the classifier parses real `srb tc` output lines too'
printf 'app/widget.rb:10: Method totall does not exist on Widget https://srb.help/7001\n' >"$SCRATCH/hit/parent-srb.txt"
printf 'No errors! Great job.\n' >"$SCRATCH/hit/fix-srb.txt"
sout=$(ruby "$HERE/judge.rb" --record="$SCRATCH/hit/record.json" \
  --parent-diags="$SCRATCH/hit/parent-srb.txt" --parent-root= \
  --fix-diags="$SCRATCH/hit/fix-srb.txt" --fix-root= --tool=srb)
slabel=$(ruby -rjson -e 'puts JSON.parse(STDIN.read)["label"]' <<<"$sout")
if [[ $slabel == HIT ]]; then pass 'srb format -> HIT'; else fail "srb format expected HIT, got $slabel"; printf '%s\n' "$sout"; fi

say 'miner — mine.rb must fire on every signal shape and stay silent on noise'
MREPO=$SCRATCH/miner/repo
mkdir -p "$MREPO/app"
git -C "$MREPO" init -q
git -C "$MREPO" config user.email t@t
git -C "$MREPO" config user.name t
cat > "$MREPO/app/widget.rb" <<'RB'
class Widget
  def price(order)
    order.total
  rescue NoMethodError
    0
  end

  def append_all(w, x, y)
    w.append(x, y)
  end
end
RB
printf 'class Invoice\nend\n\ninvoice = Invoice.new\ninvoice.totall\n' >"$MREPO/app/invoice.rb"
git -C "$MREPO" add app && git -C "$MREPO" commit -qm base
ruby -e 's = File.read(ARGV[0]); File.write(ARGV[0], s.sub("  rescue NoMethodError\n    0\n", ""))' "$MREPO/app/widget.rb"
git -C "$MREPO" add app && git -C "$MREPO" commit -qm 'Clean up price handling'
ruby -e 's = File.read(ARGV[0]); File.write(ARGV[0], s.sub("invoice.totall", "invoice.total"))' "$MREPO/app/invoice.rb"
git -C "$MREPO" add app && git -C "$MREPO" commit -qm 'Tidy invoice call'
ruby -e 's = File.read(ARGV[0]); File.write(ARGV[0], s.sub("w.append(x, y)", "w.append(x)"))' "$MREPO/app/widget.rb"
git -C "$MREPO" add app && git -C "$MREPO" commit -qm 'Adjust append arity'
printf '# a comment\n' >>"$MREPO/app/invoice.rb"
git -C "$MREPO" add app && git -C "$MREPO" commit -qm 'Add comment'
ruby -e 's = File.read(ARGV[0]); File.write(ARGV[0], s.sub("# a comment", "# another comment"))' "$MREPO/app/invoice.rb"
git -C "$MREPO" add app && git -C "$MREPO" commit -qm 'Fix NoMethodError around invoice comments'
ruby "$HERE/mine.rb" "$MREPO" selftest-miner --out "$SCRATCH/miner/mined.jsonl" 2>"$SCRATCH/miner/mine.log"
signals=$(ruby -rjson -e 'puts File.readlines(ARGV[0]).map { |l| JSON.parse(l)["signal"] }.join(",")' "$SCRATCH/miner/mined.jsonl")
if [[ $signals == 'message,single-token-edit,single-token-edit,rescue-removal' ]]; then
  pass "miner signals: $signals (comment-only diff produced nothing)"
else
  fail "miner signals expected message,single-token-edit,single-token-edit,rescue-removal, got: ${signals:-<none>}"
  cat "$SCRATCH/miner/mine.log"
fi

say 'miner --signal — the filter keeps only named signals; unknown names die'
ruby "$HERE/mine.rb" "$MREPO" selftest-miner --signal message --out "$SCRATCH/miner/mined-message.jsonl" 2>"$SCRATCH/miner/mine-message.log"
only=$(ruby -rjson -e 'puts File.readlines(ARGV[0]).map { |l| JSON.parse(l)["signal"] }.uniq.join(",")' "$SCRATCH/miner/mined-message.jsonl")
if [[ $only == 'message' ]]; then
  pass "miner --signal message -> message only"
else
  fail "miner --signal message expected message only, got: ${only:-<none>}"
fi
rc=0
ruby "$HERE/mine.rb" "$MREPO" selftest-miner --signal bogus --out "$SCRATCH/miner/mined-bogus.jsonl" 2>/dev/null || rc=$?
if [[ $rc -eq 2 ]]; then
  pass 'miner --signal bogus -> exit 2 (unknown signal dies)'
else
  fail "miner --signal bogus exited $rc, expected 2"
fi

say 'miner dedupe — a merge twin with identical hunks folds into the descriptive record'
DREPO=$SCRATCH/miner/drepo
mkdir -p "$DREPO/app"
git -C "$DREPO" init -q
git -C "$DREPO" config user.email t@t
git -C "$DREPO" config user.name t
printf 'class Invoice\nend\n\ninvoice = Invoice.new\ninvoice.totall\n' >"$DREPO/app/invoice.rb"
git -C "$DREPO" add app && git -C "$DREPO" commit -qm base
DEFBRANCH=$(git -C "$DREPO" branch --show-current)
git -C "$DREPO" checkout -qb pr
ruby -e 's = File.read(ARGV[0]); File.write(ARGV[0], s.sub("invoice.totall", "invoice.total"))' "$DREPO/app/invoice.rb"
git -C "$DREPO" add app && git -C "$DREPO" commit -qm 'Fix NoMethodError when invoice totals are blank'
git -C "$DREPO" checkout -q "$DEFBRANCH"
git -C "$DREPO" merge --no-ff -q pr -m 'Merge pull request #1 from acme/fix-totals'
ruby "$HERE/mine.rb" "$DREPO" selftest-dedupe --out "$SCRATCH/miner/deduped.jsonl" 2>"$SCRATCH/miner/dedupe.log"
verdict=$(ruby -rjson -e '
  recs = File.readlines(ARGV[0]).map { |l| JSON.parse(l) }
  if recs.size != 1
    print "expected 1 record, got #{recs.size}"
  else
    r = recs[0]
    also = r["also"] || []
    print (r["subject"].start_with?("Fix NoMethodError") && also.size == 1) ? "OK" : "kept=#{r["subject"].inspect} also=#{also.inspect}"
  end' "$SCRATCH/miner/deduped.jsonl")
if [[ $verdict == OK ]]; then
  pass 'dedupe kept the descriptive record, merge sha recorded under also'
else
  fail "dedupe: $verdict"
  cat "$SCRATCH/miner/dedupe.log"
fi

say 'summary'
if [[ $FAILED -eq 0 ]]; then
  echo "RESULT: PASS (scratch kept at $SCRATCH)"
  exit 0
fi
echo "RESULT: FAIL (scratch kept at $SCRATCH)"
exit 1

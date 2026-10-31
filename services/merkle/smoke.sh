#!/usr/bin/env bash
# Smoke test for the Merkle service (Stage 6.6).
# Runs curl against the local server and checks responses.
set -u

BASE="http://127.0.0.1:4003"
ONE="0000000000000000000000000000000000000000000000000000000000000001"
TWO="0000000000000000000000000000000000000000000000000000000000000002"
COMMITMENT="09d9d188784ab20199a5eb7267ce27765a374ce6bc672deee9eeac9ba90b80fc"

pass=0
fail=0

check() {
  local name="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "PASS  $name"
    pass=$((pass + 1))
  else
    echo "FAIL  $name"
    echo "      expected: $expected"
    echo "      actual:   $actual"
    fail=$((fail + 1))
  fi
}

echo "=== GET /health ==="
r=$(curl -s "$BASE/health")
check "/health status" '{"status":"ok"}' "$r"

echo "=== POST /hash ==="
r=$(curl -s -X POST "$BASE/hash" -H "Content-Type: application/json" \
  -d "{\"left\":\"$ONE\",\"right\":\"$TWO\"}")
check "/hash(1,2)" '{"hash":"299bfccd7daf3c917e51291383929049ec0eaed800af245056cbf135f7dea636"}' "$r"

echo "=== POST /root ==="
r=$(curl -s -X POST "$BASE/root" -H "Content-Type: application/json" \
  -d "{\"commitments\":[\"$COMMITMENT\"]}")
check "/root([commitment])" '{"root":"266ab002e3bea99195ef395e33dee02a83674155eaee9d1dc84019af70714967"}' "$r"

echo "=== POST /proof ==="
r=$(curl -s -X POST "$BASE/proof" -H "Content-Type: application/json" \
  -d "{\"commitments\":[\"$COMMITMENT\"],\"leaf_index\":0}")
proof_len=$(echo "$r" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.proof.length)})')
is_even_len=$(echo "$r" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.is_even.length)})')
check "/proof proof.length" "20" "$proof_len"
check "/proof is_even.length" "20" "$is_even_len"

echo "=== validation errors (expect 400) ==="
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/hash" -H "Content-Type: application/json" \
  -d "{\"left\":\"00\",\"right\":\"$TWO\"}")
check "/hash short hex -> 400" "400" "$code"

code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/hash" -H "Content-Type: application/json" \
  -d "{\"left\":\"0x$ONE\",\"right\":\"$TWO\"}")
check "/hash 0x-prefix -> 400" "400" "$code"

code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/proof" -H "Content-Type: application/json" \
  -d "{\"commitments\":[\"$COMMITMENT\"],\"leaf_index\":5}")
check "/proof out-of-range -> 400" "400" "$code"

echo
echo "=== summary ==="
echo "pass: $pass"
echo "fail: $fail"
[ "$fail" -eq 0 ] && exit 0 || exit 1

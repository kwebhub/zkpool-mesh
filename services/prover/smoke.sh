#!/usr/bin/env bash
# Smoke test for the prover service (Stage 7.7).
# Requires the service running on 127.0.0.1:4002.
set -u

BASE="http://127.0.0.1:4002"
REQ=/tmp/prover-smoke-req.json
RESP=/tmp/prover-smoke-resp.json

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

# --- /health ---
echo "=== GET /health ==="
r=$(curl -s "$BASE/health")
check "/health status" '{"status":"ok"}' "$r"

# --- /prove (real) ---
echo "=== POST /prove ==="
cat >"$REQ" <<'JSON'
{"root":"1e8508c3c11def8bfecd33c4bf97ce7fd065fea15eafe55c47e35bd056f1c6b2","nullifier_hash":"1412cc9d862599e6869a1881c8562062b98537456d9035819288219b5cd3e6e4","recipient":"062afbde1181c71c","recipient_binding":"200cfdb247b0436ba0483327abcf8f83ed65f75a0db8d9ebbb611561d7d3b2e1","amount":"0f4240","nullifier":"018abef7846071c7","secret":"03157def08c0e38e","note_secret":"04a03ce68d215555","merkle_proof":["07b5bad595e238e3","00","00","00","00","00","00","00","00","00","00","00","00","00","00","00","00","00","00","00"],"is_even":[true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true]}
JSON

code=$(curl -s -m 60 -o "$RESP" -w "%{http_code}" -X POST "$BASE/prove" -H "Content-Type: application/json" -d @"$REQ")
check "/prove status" "200" "$code"

proof_len=$(node -e "console.log(require('$RESP').proof.length)")
pw_len=$(node -e "console.log(require('$RESP').public_witness.length)")
check "/prove proof hex len" "648" "$proof_len"
check "/prove public_witness hex len" "344" "$pw_len"

# --- validation: wrong merkle_proof length -> 500 ---
echo "=== validation ==="
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/prove" -H "Content-Type: application/json" -d '{"root":"a","nullifier_hash":"b","recipient":"c","recipient_binding":"d","amount":"e","nullifier":"f","secret":"0","note_secret":"1","merkle_proof":["00"],"is_even":[true]}')
check "wrong merkle len -> 500" "500" "$code"

# --- malformed JSON -> 422 ---
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/prove" -H "Content-Type: application/json" -d '{}')
check "empty payload -> 422" "422" "$code"

echo
echo "=== summary ==="
echo "pass: $pass"
echo "fail: $fail"
[ "$fail" -eq 0 ] && exit 0 || exit 1

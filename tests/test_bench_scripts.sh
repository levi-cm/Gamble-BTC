#!/usr/bin/env bash
set -uo pipefail

source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

# Run copied scripts in a task-owned tree; never remove real benchmark evidence.
repo_root="$tmp_dir/repo"
mkdir -p "$repo_root/scripts"
cp "$source_root"/scripts/bench-{run,ab,current}.sh "$repo_root/scripts/"

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"

FAKE_DOCKER_LOG="$tmp_dir/docker.log"
FAKE_RM_LOG="$tmp_dir/rm.log"
touch "$FAKE_DOCKER_LOG" "$FAKE_RM_LOG"

cat >"$fake_bin/docker" <<'FAKE_DOCKER'
#!/usr/bin/env bash
printf 'docker' >>"${FAKE_DOCKER_LOG:?}"
printf ' %q' "$@" >>"${FAKE_DOCKER_LOG:?}"
printf '\n' >>"${FAKE_DOCKER_LOG:?}"

case "${1:-}" in
  build)
    exit 0
    ;;
  run)
    if [[ "${FAKE_DOCKER_FAIL:-0}" == "1" ]]; then
      exit "${FAKE_DOCKER_EXIT:-1}"
    fi
    if [[ -n "${FAKE_DOCKER_SLEEP:-}" ]]; then
      sleep "${FAKE_DOCKER_SLEEP}"
    fi
    if [[ -n "${FAKE_DOCKER_OUTPUT:-}" && -f "${FAKE_DOCKER_OUTPUT}" ]]; then
      cat "${FAKE_DOCKER_OUTPUT}"
    fi
    exit "${FAKE_DOCKER_EXIT:-0}"
    ;;
  rm)
    printf 'docker' >>"${FAKE_RM_LOG:?}"
    printf ' %q' "$@" >>"${FAKE_RM_LOG:?}"
    printf '\n' >>"${FAKE_RM_LOG:?}"
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
FAKE_DOCKER

chmod +x "$fake_bin/docker"

VALID_JSON="$tmp_dir/valid.json"
cat >"$VALID_JSON" <<'EOF'
{"ok":true,"backend":"opencl","mh_s":42.5,"seconds":1.0,"hashes":100}
EOF

MALFORMED_JSON="$tmp_dir/bad.json"
printf '{bad json\n' >"$MALFORMED_JSON"

touch "$tmp_dir/card" "$tmp_dir/render"

run_bench_run() {
  env \
    PATH="$fake_bin:$PATH" \
    FAKE_DOCKER_LOG="$FAKE_DOCKER_LOG" \
    FAKE_RM_LOG="$FAKE_RM_LOG" \
    "$@"
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" != *"$needle"* ]]; then
    printf 'expected to contain: %s\nactual:\n%s\n' "$needle" "$haystack" >&2
    exit 1
  fi
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" == *"$needle"* ]]; then
    printf 'expected not to contain: %s\nactual:\n%s\n' "$needle" "$haystack" >&2
    exit 1
  fi
}

# Reset logs helper
reset_logs() {
  : >"$FAKE_DOCKER_LOG"
  : >"$FAKE_RM_LOG"
}

# 1. backend/image/env forwarding
reset_logs
out_dir="$tmp_dir/br1"
mkdir -p "$out_dir"
# run in repo root but redirect outputs? script computes bench-results relative cwd; use repo root
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$VALID_JSON" \
  GBTC_BACKEND=opencl \
  GBTC_IMAGE=testimage:latest \
  GBTC_VULKAN_LOCAL_SIZE=128 \
  GBTC_OPENCL_LOCAL_SIZE=32 \
  GBTC_GLES_KERNEL=altbool \
  GBTC_BENCH_SECONDS=1 \
  GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_CONTAINER_NAME=gbtc-bench-test \
  GBTC_DRI_CARD="$tmp_dir/card" \
  GBTC_DRI_RENDER="$tmp_dir/render" \
  GBTC_VIDEO_GID=1 \
  GBTC_RENDER_GID=2 \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br1.out 2>"$tmp_dir"/br1.err
)
rc=$?
if [[ $rc -ne 0 ]]; then
  printf 'bench-run forwarding test failed rc=%s\nstderr:\n%s\n' "$rc" "$(cat "$tmp_dir"/br1.err)" >&2
  exit 1
fi
docker_log="$(cat "$FAKE_DOCKER_LOG")"
assert_contains "$docker_log" "GBTC_BACKEND=opencl"
assert_contains "$docker_log" "testimage:latest"
assert_contains "$docker_log" "GBTC_VULKAN_LOCAL_SIZE=128"
assert_contains "$docker_log" "GBTC_OPENCL_LOCAL_SIZE=32"
assert_not_contains "$docker_log" "GBTC_GLES_KERNEL=altbool"
assert_contains "$docker_log" "--name gbtc-bench-test"
assert_contains "$docker_log" "GBTC_DEVICE=$tmp_dir/render"

# 1b. gles backend retains GLES injections
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$VALID_JSON" \
  GBTC_BACKEND=gles \
  GBTC_IMAGE=testimage:latest \
  GBTC_GLES_KERNEL=altbool \
  GBTC_BENCH_SECONDS=1 \
  GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_CONTAINER_NAME=gbtc-bench-gles \
  GBTC_DRI_CARD="$tmp_dir/card" \
  GBTC_DRI_RENDER="$tmp_dir/render" \
  GBTC_VIDEO_GID=1 \
  GBTC_RENDER_GID=2 \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br1b.out 2>"$tmp_dir"/br1b.err
)
rc=$?
if [[ $rc -ne 0 ]]; then
  printf 'bench-run gles forwarding test failed rc=%s\nstderr:\n%s\n' "$rc" "$(cat "$tmp_dir"/br1b.err)" >&2
  exit 1
fi
docker_log="$(cat "$FAKE_DOCKER_LOG")"
assert_contains "$docker_log" "GBTC_GLES_KERNEL=altbool"

# 2. failing container exit leads nonzero
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_FAIL=1 FAKE_DOCKER_EXIT=7 \
  GBTC_BENCH_SECONDS=1 GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br2.out 2>"$tmp_dir"/br2.err
)
rc=$?
if [[ $rc -eq 0 ]]; then
  printf 'expected nonzero on failing container\n' >&2
  exit 1
fi

# 3. timeout cleanup exact container
reset_logs
container_name="gbtc-bench-timeout-$$"
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_SLEEP=5 \
  GBTC_BENCH_SECONDS=1 GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=1 \
  GBTC_CONTAINER_NAME="$container_name" \
  GBTC_DRI_CARD="$tmp_dir/card" GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br3.out 2>"$tmp_dir"/br3.err
)
rc=$?
if [[ $rc -eq 0 ]]; then
  printf 'expected nonzero on timeout\n' >&2
  exit 1
fi
rm_log="$(cat "$FAKE_RM_LOG")"
assert_contains "$rm_log" "$container_name"
# no broad process kill
if grep -RniE 'pkill|killall' "$repo_root/scripts/bench-run.sh" "$repo_root/scripts/bench-ab.sh" "$repo_root/scripts/bench-current.sh" >/dev/null; then
  printf 'broad process kill found\n' >&2
  exit 1
fi

# 4. output validation
# missing output
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  GBTC_BENCH_SECONDS=1 GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br4.out 2>"$tmp_dir"/br4.err
)
rc=$?
if [[ $rc -eq 0 ]]; then
  printf 'expected nonzero on missing output\n' >&2
  exit 1
fi
# malformed output
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$MALFORMED_JSON" \
  GBTC_BENCH_SECONDS=1 GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br5.out 2>"$tmp_dir"/br5.err
)
rc=$?
if [[ $rc -eq 0 ]]; then
  printf 'expected nonzero on malformed output\n' >&2
  exit 1
fi
# empty output file rejected (simulate by empty fixture)
EMPTY_JSON="$tmp_dir/empty.json"
: >"$EMPTY_JSON"
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$EMPTY_JSON" \
  GBTC_BENCH_SECONDS=1 GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-run.sh" >"$tmp_dir"/br6.out 2>"$tmp_dir"/br6.err
)
rc=$?
if [[ $rc -eq 0 ]]; then
  printf 'expected nonzero on empty output\n' >&2
  exit 1
fi

# 5. bench-ab explicit backend selection + balanced order
reset_logs
rm -f "$repo_root"/bench-results/ab-*.tsv "$repo_root"/bench-results/ab-*.order 2>/dev/null || true
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$VALID_JSON" \
  GBTC_BACKEND=vulkan \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" \
  GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-ab.sh" 2 1 \
    "A:GBTC_BACKEND=opencl" \
    "B:GBTC_BACKEND=gles" >"$tmp_dir"/ba.out 2>"$tmp_dir"/ba.err
)
rc=$?
if [[ $rc -ne 0 ]]; then
  printf 'bench-ab failed rc=%s\nstderr:\n%s\n' "$rc" "$(cat "$tmp_dir"/ba.err)" >&2
  exit 1
fi
ba_log="$(cat "$FAKE_DOCKER_LOG")"
# Balanced order ABBA: opencl, gles, gles, opencl
assert_contains "$ba_log" "GBTC_BACKEND=opencl"
assert_contains "$ba_log" "GBTC_BACKEND=gles"
# Check sequence by extracting backend order
seq_backends="$(printf '%s\n' "$ba_log" | sed -n 's/.*GBTC_BACKEND=\([^ ]*\).*/\1/p')"
# Expect 4 invocations
count="$(printf '%s\n' "$seq_backends" | wc -l)"
if [[ "$count" -ne 4 ]]; then
  printf 'expected 4 bench invocations, got %s\n%s\n' "$count" "$seq_backends" >&2
  exit 1
fi
expected="opencl
gles
gles
opencl"
if [[ "$seq_backends" != "$expected" ]]; then
  printf 'expected balanced order opencl,gles,gles,opencl; got:\n%s\n' "$seq_backends" >&2
  exit 1
fi
# order file recorded
order_files=("$repo_root"/bench-results/ab-*.order)
if [[ ! -f "${order_files[0]}" ]]; then
  printf 'missing order file\n' >&2
  exit 1
fi
# default backend honored when not forced: global vulkan should appear if no override
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$VALID_JSON" \
  GBTC_BACKEND=vulkan \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" \
  GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-ab.sh" 1 1 "X:" >"$tmp_dir"/ba2.out 2>"$tmp_dir"/ba2.err
)
rc=$?
if [[ $rc -ne 0 ]]; then
  printf 'bench-ab default backend test failed rc=%s\nstderr:\n%s\n' "$rc" "$(cat "$tmp_dir"/ba2.err)" >&2
  exit 1
fi
ba2_log="$(cat "$FAKE_DOCKER_LOG")"
assert_contains "$ba2_log" "GBTC_BACKEND=vulkan"

# 6. telemetry missing -> unknown not zero
reset_logs
# Use a card name without sysfs telemetry
GBTC_DRI_CARD_TEST="/dev/dri/card999999"
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$VALID_JSON" \
  GBTC_BACKEND=opencl \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$GBTC_DRI_CARD_TEST" \
  GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-ab.sh" 1 1 "T:" >"$tmp_dir"/ba3.out 2>"$tmp_dir"/ba3.err
)
rc=$?
if [[ $rc -ne 0 ]]; then
  printf 'bench-ab telemetry test failed rc=%s\nstderr:\n%s\n' "$rc" "$(cat "$tmp_dir"/ba3.err)" >&2
  exit 1
fi
tsv_file="$(ls "$repo_root"/bench-results/ab-*.tsv | head -n1)"
tsv_content="$(cat "$tsv_file")"
assert_contains "$tsv_content" "unknown"
if printf '%s\n' "$tsv_content" | grep -Eq '(^|	)0(	|$)' ; then
  # allow zeros in round/label? Check telemetry columns specifically: freq_min should not be 0 due to missing
  :
fi
# Ensure no telemetry column is exactly 0 where unknown expected: freq_min column (6) not 0
freq_col="$(printf '%s\n' "$tsv_content" | awk -F'\t' 'NR>1 && $1!="round" {print $6}' | head -n1)"
if [[ "$freq_col" == "0" ]]; then
  printf 'missing telemetry recorded as 0 instead of unknown\n%s\n' "$tsv_content" >&2
  exit 1
fi

# bench-current conversion sanity
reset_logs
(
  cd "$repo_root"
  rm -rf bench-results
  mkdir -p bench-results
  FAKE_DOCKER_OUTPUT="$VALID_JSON" \
  GBTC_BACKEND=gles \
  GBTC_BENCH_SECONDS=1 GBTC_BENCH_WARMUP_SECONDS=1 \
  GBTC_BENCH_TIMEOUT_SECONDS=10 \
  GBTC_DRI_CARD="$tmp_dir/card" GBTC_DRI_RENDER="$tmp_dir/render" \
  run_bench_run "$repo_root/scripts/bench-current.sh" >"$tmp_dir"/bc.out 2>"$tmp_dir"/bc.err
)
rc=$?
if [[ $rc -ne 0 ]]; then
  printf 'bench-current failed rc=%s\nstderr:\n%s\n' "$rc" "$(cat "$tmp_dir"/bc.err)" >&2
  exit 1
fi

printf 'bench script tests passed\n'
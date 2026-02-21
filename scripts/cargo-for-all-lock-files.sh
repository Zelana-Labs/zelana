#!/usr/bin/env bash
set -e

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
  >&2 echo "cargo not found! Please install Rust and Cargo first."
  exit 1
fi

# Parse optional --ignore-exit-code
ignore_exit_code=0
shifted_args=()

while [[ -n $1 ]]; do
  case $1 in
    --ignore-exit-code)
      ignore_exit_code=1
      shift
      ;;
    --)
      shift
      break
      ;;
    *)
      shifted_args+=("$1")
      shift
      ;;
  esac
done

# Get all Cargo.lock files in the repo
lock_files=$(git ls-files '**/Cargo.lock')

# Loop over all crates
for lock_file in $lock_files; do
  crate_dir=$(dirname "$lock_file")
  
  if [[ -n $CI ]]; then
    echo "--- [$crate_dir]: cargo ${shifted_args[*]} $@"
  fi

  # Run cargo command in the crate directory
  if (set -x && cd "$crate_dir" && cargo "${shifted_args[@]}" "$@"); then
    # success
    true
  else
    failed_exit_code=$?
    if [[ $ignore_exit_code -eq 1 ]]; then
      echo "WARN: Ignoring failed cargo command in $crate_dir (exit code $failed_exit_code)"
      true
    else
      exit $failed_exit_code
    fi
  fi
done
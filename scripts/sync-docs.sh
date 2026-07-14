#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SHARED_CORE="$ROOT_DIR/docs/content/shared-core.md"
BEGIN_MARKER='<!-- BEGIN SHARED:core -->'
END_MARKER='<!-- END SHARED:core -->'

if [ ! -f "$SHARED_CORE" ]; then
  echo "Missing shared content file: $SHARED_CORE" >&2
  exit 1
fi

update_file() {
  target="$1"

  if ! grep -Fq "$BEGIN_MARKER" "$target"; then
    echo "Missing begin marker in $target" >&2
    exit 1
  fi

  if ! grep -Fq "$END_MARKER" "$target"; then
    echo "Missing end marker in $target" >&2
    exit 1
  fi

  tmp_file=$(mktemp)

  awk -v shared_file="$SHARED_CORE" -v begin="$BEGIN_MARKER" -v end="$END_MARKER" '
    $0 == begin {
      print
      while ((getline line < shared_file) > 0) {
        print line
      }
      close(shared_file)
      skipping = 1
      next
    }
    $0 == end {
      skipping = 0
      print
      next
    }
    !skipping {
      print
    }
  ' "$target" > "$tmp_file"

  mv "$tmp_file" "$target"
}

update_file "$ROOT_DIR/README.md"
update_file "$ROOT_DIR/index.md"

echo "Synchronized shared docs content into README.md and index.md"

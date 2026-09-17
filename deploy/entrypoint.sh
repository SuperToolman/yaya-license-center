#!/bin/sh
set -eu

for secret in private.pem public.pem admin-password.txt; do
  test -f "/run/secrets/$secret" || { echo "Missing /run/secrets/$secret" >&2; exit 2; }
done

export LICENSE_SIGNING_PRIVATE_KEY_PEM="$(cat /run/secrets/private.pem)"
export LICENSE_SIGNING_PUBLIC_KEY_PEM="$(cat /run/secrets/public.pem)"
export YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME="${YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME:-admin}"
export YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD="$(cat /run/secrets/admin-password.txt)"

database_dir=/var/lib/yaya-operation-center
database_path="$database_dir/operation-center.sqlite3"
legacy_database_path="$database_dir/license-center.sqlite3"
if [ ! -f "$database_path" ] && [ -f "$legacy_database_path" ]; then
  cp "$legacy_database_path" "$database_path"
  for suffix in -wal -shm; do
    test -f "$legacy_database_path$suffix" && cp "$legacy_database_path$suffix" "$database_path$suffix" || true
  done
fi

/app/api/yaya-operation-center-api &
api_pid=$!

cleanup() {
  kill "$api_pid" "$web_pid" 2>/dev/null || true
  wait "$api_pid" 2>/dev/null || true
  wait "$web_pid" 2>/dev/null || true
}
trap cleanup INT TERM

node /app/web/server.js &
web_pid=$!

set +e
wait "$web_pid"
web_status=$?
cleanup
exit "$web_status"

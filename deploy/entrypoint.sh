#!/bin/sh
set -eu

for secret in private.pem public.pem admin-token.txt; do
  test -f "/run/secrets/$secret" || { echo "Missing /run/secrets/$secret" >&2; exit 2; }
done

export LICENSE_SIGNING_PRIVATE_KEY_PEM="$(cat /run/secrets/private.pem)"
export LICENSE_SIGNING_PUBLIC_KEY_PEM="$(cat /run/secrets/public.pem)"
export LICENSE_CENTER_ADMIN_TOKEN="$(cat /run/secrets/admin-token.txt)"

/app/api/yaya-license-center-api &
api_pid=$!

cleanup() {
  kill "$api_pid" 2>/dev/null || true
  wait "$api_pid" 2>/dev/null || true
}
trap cleanup INT TERM EXIT

exec node /app/web/server.js

#!/bin/sh
set -eu

certificate=/var/lib/postgresql/server.crt
private_key=/var/lib/postgresql/server.key
trust_directory=/var/run/sdkwork-postgres-tls
shared_certificate=$trust_directory/server.crt

mkdir -p "$trust_directory"

if [ ! -s "$certificate" ] || [ ! -s "$private_key" ]; then
  certificate_tmp="${certificate}.tmp.$$"
  private_key_tmp="${private_key}.tmp.$$"
  trap 'rm -f "$certificate_tmp" "$private_key_tmp"' EXIT HUP INT TERM
  original_umask=$(umask)
  umask 077
  openssl req -new -x509 -nodes -days 3650 \
    -subj '/CN=postgres' \
    -keyout "$private_key_tmp" \
    -out "$certificate_tmp" >/dev/null 2>&1
  chown postgres:postgres "$certificate_tmp" "$private_key_tmp"
  chmod 0644 "$certificate_tmp"
  chmod 0600 "$private_key_tmp"
  mv -f "$certificate_tmp" "$certificate"
  mv -f "$private_key_tmp" "$private_key"
  umask "$original_umask"
  trap - EXIT HUP INT TERM
fi

shared_certificate_tmp="${shared_certificate}.tmp.$$"
trap 'rm -f "$shared_certificate_tmp"' EXIT HUP INT TERM
cp "$certificate" "$shared_certificate_tmp"
chmod 0644 "$shared_certificate_tmp"
mv -f "$shared_certificate_tmp" "$shared_certificate"
trap - EXIT HUP INT TERM

exec /usr/local/bin/docker-entrypoint.sh "$@"

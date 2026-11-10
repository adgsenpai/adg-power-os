#!/bin/sh
# Acceptance A/B (generator half): the eco generator masks nothing on a
# normal command line and masks the listed units (never NetworkManager) with
# the eco marker. Runs without root against temporary directories.
set -eu
HERE=$(cd "$(dirname "$0")/.." && pwd)
GEN="$HERE/system/generators/adg-power-eco-generator"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/conf" "$T/normal" "$T/early" "$T/late"
printf 'docker.service\ndocker.socket\n# comment\nNetworkManager.service\nbad;name.service\npostgresql.service\n' > "$T/conf/eco-mask-system.list"
fail() { echo "FAIL: $*"; exit 1; }

echo "BOOT_IMAGE=/vmlinuz ro quiet splash" > "$T/cmdline"
ADG_POWER_CMDLINE="$T/cmdline" ADG_POWER_CONF_DIR="$T/conf" sh "$GEN" "$T/normal" "$T/early" "$T/late"
[ -z "$(ls -A "$T/early")" ] || fail "normal boot masked units"
echo "ok - normal boot: nothing masked"

echo "BOOT_IMAGE=/vmlinuz ro quiet splash adgpower.eco=1" > "$T/cmdline"
ADG_POWER_CMDLINE="$T/cmdline" ADG_POWER_CONF_DIR="$T/conf" sh "$GEN" "$T/normal" "$T/early" "$T/late"
for u in docker.service docker.socket postgresql.service; do
    [ "$(readlink "$T/early/$u")" = /dev/null ] || fail "$u not masked"
done
[ ! -e "$T/early/NetworkManager.service" ] || fail "NetworkManager masked"
[ ! -e "$T/early/bad;name.service" ] || fail "unsafe name accepted"
[ -z "$(ls -A "$T/normal")" ] || fail "wrote outside the early dir"
echo "ok - eco boot: containers/databases masked, NetworkManager kept"

echo "adgpower.eco=10" > "$T/cmdline"; rm -f "$T/early"/*
ADG_POWER_CMDLINE="$T/cmdline" ADG_POWER_CONF_DIR="$T/conf" sh "$GEN" "$T/normal" "$T/early" "$T/late"
[ -z "$(ls -A "$T/early")" ] || fail "partial marker matched"
echo "ok - marker must match exactly"

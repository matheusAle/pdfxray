#!/usr/bin/env bash
# Cloudflare protection and analytics for pdfxray.app (Free plan). Idempotent: every run replaces
# the zone's custom firewall rules, rate limiting rules and cache rules with the ones below, and
# creates the Web Analytics site only if it does not exist yet.
#
#   dl.pdfxray.app (R2):
#     firewall  only GET/HEAD, only /releases/, /latest/ and /appcast.xml, no empty user agent
#     rate      30 requests per 10 s per IP and data center, then blocked for 10 s
#     cache     cached at the edge with the objects' own Cache-Control (immutable releases,
#               5 minutes for latest/ and the update feed)
#   pdfxray.app:
#     analytics a Web Analytics site; the beacon token is printed for site/index.html
#
# Bot Fight Mode is deliberately NOT enabled: on the Free plan it applies to the whole zone and
# cannot be skipped per host, and its challenges would break Sparkle's update checks and
# downloads with curl, which are not browsers.
#
# Usage: CLOUDFLARE_SETUP_TOKEN_FILE=~/.config/pdfxray/cloudflare-setup-token \
#          packaging/cloudflare/protect-downloads.sh [--dry-run]
# The token needs, for the pdfxray.app zone: Zone WAF Edit and Cache Rules Edit; for the account:
# Web Analytics Edit. The token is read from the file and never printed.

set -euo pipefail

ACCOUNT=d16dfc846cc4d6600745e57b7cebe94c
ZONE=7981f10078ec217fd3e3da7fa1bf2f1b
DL_HOST=dl.pdfxray.app
SITE_HOST=pdfxray.app

dry_run=false
[ "${1:-}" = "--dry-run" ] && dry_run=true

token_file="${CLOUDFLARE_SETUP_TOKEN_FILE:-$HOME/.config/pdfxray/cloudflare-setup-token}"
if ! $dry_run; then
  [ -r "$token_file" ] || { echo "no token file at $token_file" >&2; exit 1; }
  TOKEN="$(tr -d '[:space:]' < "$token_file")"
fi

api() { # method path [json]
  local method=$1 path=$2 body=${3:-}
  if $dry_run; then
    { echo "== $method $path"; if [ -n "$body" ]; then jq . <<<"$body"; fi; } >&2; return 0
  fi
  local out
  out="$(curl -sS -X "$method" "https://api.cloudflare.com/client/v4$path" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    ${body:+--data "$body"})"
  if [ "$(jq -r .success <<<"$out")" != true ]; then
    echo "$method $path failed: $(jq -c .errors <<<"$out")" >&2
    return 1
  fi
  printf '%s' "$out"
}

dl="http.host eq \"$DL_HOST\""
known_paths='(starts_with(http.request.uri.path, "/releases/") or starts_with(http.request.uri.path, "/latest/") or http.request.uri.path eq "/appcast.xml")'

firewall=$(jq -n --arg dl "$dl" --arg known "$known_paths" '{rules: [
  {description: "dl: only GET and HEAD", action: "block", enabled: true,
   expression: "(\($dl) and not http.request.method in {\"GET\" \"HEAD\"})"},
  {description: "dl: only release files and the update feed", action: "block", enabled: true,
   expression: "(\($dl) and not \($known))"},
  {description: "dl: no empty user agent", action: "block", enabled: true,
   expression: "(\($dl) and http.user_agent eq \"\")"}
]}')

# Free plan: one rule, a 10 s period and a 10 s block, counted per IP and data center. The
# expression can only use the path, so it names the download paths, which the site does not use.
ratelimit=$(jq -n --arg known "$known_paths" '{rules: [
  {description: "dl: 30 requests per 10 s per IP", action: "block", enabled: true,
   expression: $known,
   ratelimit: {characteristics: ["cf.colo.id", "ip.src"], period: 10,
               requests_per_period: 30, mitigation_timeout: 10}}
]}')

cache=$(jq -n --arg dl "$dl" '{rules: [
  {description: "dl: cache with the objects own Cache-Control", action: "set_cache_settings",
   enabled: true, expression: "(\($dl))",
   action_parameters: {cache: true, edge_ttl: {mode: "respect_origin"},
                       browser_ttl: {mode: "respect_origin"}}}
]}')

api PUT "/zones/$ZONE/rulesets/phases/http_request_firewall_custom/entrypoint" "$firewall" >/dev/null
echo "firewall rules: set"
api PUT "/zones/$ZONE/rulesets/phases/http_ratelimit/entrypoint" "$ratelimit" >/dev/null
echo "rate limiting rule: set"
api PUT "/zones/$ZONE/rulesets/phases/http_request_cache_settings/entrypoint" "$cache" >/dev/null
echo "cache rule: set"

if $dry_run; then
  echo "== Web Analytics: create $SITE_HOST if missing"; exit 0
fi
sites="$(api GET "/accounts/$ACCOUNT/rum/site_info/list?per_page=100")"
site_token="$(jq -r --arg h "$SITE_HOST" '.result[] | select(.ruleset.zone_name == $h or .host == $h) | .site_token' <<<"$sites" | head -1)"
if [ -z "$site_token" ]; then
  created="$(api POST "/accounts/$ACCOUNT/rum/site_info" "$(jq -n --arg h "$SITE_HOST" '{host: $h, auto_install: false}')")"
  site_token="$(jq -r .result.site_token <<<"$created")"
  echo "Web Analytics site: created"
else
  echo "Web Analytics site: exists"
fi
# Public by design: it is embedded in the page.
echo "beacon token: $site_token"

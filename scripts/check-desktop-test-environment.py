#!/usr/bin/env python3
"""Check public desktop routes without credentials or paid requests."""
import argparse
import json
import secrets
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--web-origin', default='https://test.tokensapi.cn')
parser.add_argument('--console-api', default='https://console-api-test.tokensapi.cn')
parser.add_argument('--gateway', default='https://api-test.tokensapi.cn')
parser.add_argument('--auth-create', action='store_true', help='Create one anonymous, short-lived authorization request')
args = parser.parse_args()
failed = False


def request(label, url, data=None, expected=(200,)):
    global failed
    req = Request(url, data=data, headers={'Content-Type': 'application/json'} if data else {})
    try:
        with urlopen(req, timeout=10) as response:
            status, content_type, body = response.status, response.headers.get('Content-Type', ''), response.read(65536)
    except HTTPError as error:
        status, content_type, body = error.code, error.headers.get('Content-Type', ''), error.read(65536)
    except (URLError, TimeoutError, OSError) as error:
        print(f'{label}: unavailable ({type(error).__name__})')
        failed = True
        return None
    ok = status in expected
    print(f'{label}: HTTP {status}, content-type {content_type or "unknown"} ({"ok" if ok else "failed"})')
    failed |= not ok
    return status, content_type, body


for origin in (args.web_origin, args.console_api, args.gateway):
    parsed = urlparse(origin)
    if parsed.scheme != 'https' or not parsed.netloc or parsed.path not in ('', '/') or parsed.query or parsed.fragment:
        parser.error('Origins must be HTTPS origins without paths or query parameters')

request('WebAPI status', args.console_api.rstrip('/') + '/api/status')
for path in ('authorize', 'console', 'recharge'):
    request('Web page ' + path, args.web_origin.rstrip('/') + '/desktop/' + path)
request('Gateway reachability (no key)', args.gateway.rstrip('/') + '/v1/models', expected=(200, 401, 403))
if args.auth_create:
    body = json.dumps({'state': secrets.token_urlsafe(32), 'code_challenge': secrets.token_urlsafe(32), 'code_challenge_method': 'S256'}).encode()
    result = request('Desktop authorization create', args.console_api.rstrip('/') + '/api/desktop/auth/sessions', body)
    if result and result[0] == 200:
        try:
            data = json.loads(result[2])
            authorization = (data.get('data') or {}) if data.get('success') else {}
            actual = urlparse(authorization.get('authorize_url', ''))
            expected = urlparse(args.web_origin)
            ok = bool(authorization.get('session_id')) and (actual.scheme, actual.netloc, actual.path) == (expected.scheme, expected.netloc, '/desktop/authorize')
            print('Authorization origin and response:', 'ok' if ok else 'failed')
            failed |= not ok
        except (ValueError, TypeError, AttributeError):
            print('Authorization response: invalid')
            failed = True
else:
    print('Desktop authorization create: skipped (use --auth-create to create a temporary request)')
print('This check does not verify installed-app login, account permissions, model access, or payments.')
sys.exit(1 if failed else 0)

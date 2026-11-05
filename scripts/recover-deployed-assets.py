"""Restore exported GLBs from Jumpman's existing Vite deployment (stdlib only)."""
import argparse
import getpass
import http.cookiejar
import re
import struct
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urljoin, urlsplit
from urllib.request import HTTPCookieProcessor, Request, build_opener

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_URL = 'https://jumpman.alexandria-rnaud.workers.dev'


def is_login_page(html):
    return bool(re.search(r'<form\b[^>]*action=["\']/__login(?:\?[^"\']*)?["\']', html, re.I))


def http_failure(error, body):
    title = re.search(r'<title[^>]*>(.*?)</title>', body, re.I | re.S)
    title = re.sub(r'<[^>]*>', '', title[1]).strip()[:160] if title else '(no page title)'
    ray = error.headers.get('CF-Ray', 'not provided')
    challenge = error.headers.get('cf-mitigated') == 'challenge'
    summary = re.sub(r'<(script|style)\b[^>]*>.*?</\1>', '', body, flags=re.I | re.S)
    summary = ' '.join(re.sub(r'<[^>]*>', ' ', summary).split())[:600]
    summary = ''.join(c for c in summary if c.isprintable())
    authentication = error.headers.get('WWW-Authenticate', '(none)')
    return (f'HTTP {error.code} from {urlsplit(error.url).hostname}. Page: {title}. '
            f'Cloudflare Ray ID: {ray}. '
            f'Authentication: {authentication}. Response: {summary or "(empty body)"}. '
            + ('Cloudflare requires browser verification. ' if challenge else '')
            + 'This is not the recognized Jumpman login page. Open the deployment in your browser '
            'to check access; no password was sent for this response.')


def asset_mapping(source):
    # Vite's eager ?url imports become string constants referenced by the glob map.
    constants = dict(re.findall(r'\b([\w$]+)\s*=\s*["\'](/assets/[^"\']+\.glb)["\']', source))
    result = {}
    pattern = r'["\'](?:\.\./)*assets/((?:actors|levels)/[^"\']+\.glb)["\']\s*:\s*(?:["\']([^"\']+)["\']|([\w$]+))'
    for path, literal, variable in re.findall(pattern, source):
        url = literal or constants.get(variable)
        if not url:
            raise ValueError(f'Cannot resolve deployed URL for {path}')
        if '..' in Path(path).parts or not url.startswith('/assets/'):
            raise ValueError(f'Unexpected asset mapping: {path}')
        result[path] = url
    return result


def validate_glb(data):
    if len(data) < 12 or data[:4] != b'glTF':
        raise ValueError('Response is not a GLB file')
    version, length = struct.unpack_from('<II', data, 4)
    if version != 2 or length != len(data):
        raise ValueError('Invalid GLB version or truncated download')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default=DEFAULT_URL)
    args = parser.parse_args()
    base = args.url.rstrip('/') + '/'
    if urlsplit(base).scheme != 'https':
        parser.error('Use an HTTPS deployment URL')
    opener = build_opener(HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def fetch(path, data=None):
        url = urljoin(base, path)
        if urlsplit(url).netloc != urlsplit(base).netloc:
            raise ValueError('Refusing a cross-origin asset URL')
        request = Request(url, data=data, headers={'Origin': base.rstrip('/')})
        with opener.open(request, timeout=60) as response:
            return response.read()

    try:
        html = fetch('/').decode()
    except HTTPError as error:
        html = error.read().decode(errors='replace')
        if error.code not in (401, 403) or not is_login_page(html):
            raise ValueError(http_failure(error, html)) from None
    if is_login_page(html):
        password = getpass.getpass('Jumpman game password (not your Cloudflare password): ')
        try:
            html = fetch('/__login?next=/', urlencode({'password': password}).encode()).decode()
        except HTTPError as error:
            raise ValueError(f'Game login returned HTTP {error.code}. Check the game password and browser access.') from None
        finally:
            del password
        if is_login_page(html):
            raise ValueError('Login did not establish a session; the site returned its login page again.')

    queue = re.findall(r'(?:src|href)=["\']([^"\']+\.js)["\']', html)
    seen = set()
    mapping = {}
    while queue:
        url = urljoin(base, queue.pop())
        if url in seen:
            continue
        seen.add(url)
        source = fetch(url).decode()
        mapping.update(asset_mapping(source))
        # Follow emitted JS chunks; never execute the downloaded JavaScript.
        for child in re.findall(r'["\']((?:\./|/assets/)[^"\'\s]+\.js)["\']', source):
            queue.append(urljoin(url, child))
    if not mapping:
        raise ValueError('No original GLB paths found. No local files were changed.')
    print(f'Found {len(mapping)} exported models in the deployed bundle.')
    restored = skipped = 0
    for path, url in sorted(mapping.items()):
        destination = ROOT / 'assets' / path
        if destination.exists():
            validate_glb(destination.read_bytes())
            skipped += 1
            continue
        data = fetch(url)
        validate_glb(data)
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = destination.with_suffix('.glb.part')
        temporary.write_bytes(data)
        temporary.replace(destination)
        restored += 1
        print(f'[{restored + skipped}/{len(mapping)}] {path}')
    print(f'Restored {restored} GLBs; preserved {skipped} existing GLBs.')
    print('Existing course JSON files were preserved. Run: node scripts/check-deploy-assets.mjs')


if __name__ == '__main__':
    try:
        main()
    except (URLError, ValueError, OSError) as error:
        print(f'Recovery stopped: {error}', file=sys.stderr)
        sys.exit(1)

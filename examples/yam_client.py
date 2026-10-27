#!/usr/bin/env python3
"""Call through YAM. All UI clients must serialize invocations of this package."""
import json
import threading
import time
import urllib.request
import uuid


class YamWakewordClient:
    def __init__(self, url):
        self.url = url.rstrip('/')
        self._lock = threading.Lock()

    def _request(self, path, data=None):
        body = None if data is None else json.dumps(data).encode()
        request = urllib.request.Request(
            self.url + path, data=body,
            headers={} if body is None else {'Content-Type': 'application/json'},
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            return response.read().decode('utf-8')

    def call(self, command, *args, timeout=60):
        with self._lock:
            packages = json.loads(self._request('/api/plugins'))
            plugin = next(item for item in packages if item['manifest']['id'] == 'wakeword-cli')
            if plugin['running']:
                raise RuntimeError('Wakeword CLI is busy in YAM; do not interrupt it with another invocation')
            request_id = uuid.uuid4().hex
            self._request('/api/plugins/wakeword-cli/invoke', {
                'args': ['--json', '--request-id', request_id, command, *args],
            })
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                logs = self._request('/api/plugins/wakeword-cli/logs')
                for line in logs.splitlines():
                    try:
                        response = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if not isinstance(response, dict):
                        continue
                    if response.get('api_version') != 1 or response.get('request_id') != request_id:
                        continue
                    if not response['ok']:
                        raise RuntimeError(f"{response['error']['code']}: {response['error']['message']}")
                    # YAM may still be finishing log pumps; wait before permitting the next call.
                    while time.monotonic() < deadline:
                        packages = json.loads(self._request('/api/plugins'))
                        if not any(p['manifest']['id'] == 'wakeword-cli' and p['running'] for p in packages):
                            return response['data']
                        time.sleep(0.2)
                    raise TimeoutError('Command returned a result but YAM is still finishing it')
                time.sleep(0.25)
            raise TimeoutError(f'No result for {request_id}; command was not cancelled. Check YAM logs before retrying')


if __name__ == '__main__':
    import sys
    if len(sys.argv) < 2:
        raise SystemExit('usage: yam_client.py http://HOST:8099 [command] [args...]')
    command = sys.argv[2] if len(sys.argv) > 2 else 'list'
    print(json.dumps(YamWakewordClient(sys.argv[1]).call(command, *sys.argv[3:]), ensure_ascii=False, indent=2))

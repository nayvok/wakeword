#!/usr/bin/env python3
"""Run on the speaker. Import WakewordClient from a UI backend."""
import json
import subprocess
import uuid


class WakewordError(RuntimeError):
    def __init__(self, response):
        self.response = response
        super().__init__(response['error']['message'])


class WakewordClient:
    def __init__(self, binary='/data/quazar/yam/data/plugins/wakeword-cli/wakeword'):
        self.binary = binary

    def call(self, command, *args, timeout=60):
        request_id = uuid.uuid4().hex
        result = subprocess.run(
            [self.binary, '--json', '--request-id', request_id, command, *args],
            capture_output=True, text=True,
            # Killing apply midway can leave quasar stopped. Let mutations finish.
            timeout=None if command in ('apply', 'switch') else timeout, check=False,
        )
        response = json.loads(result.stdout)
        if response.get('api_version') != 1 or response.get('request_id') != request_id:
            raise RuntimeError('Unexpected Wakeword response')
        if not response['ok']:
            raise WakewordError(response)
        if result.returncode:
            raise RuntimeError(f'Unexpected exit code: {result.returncode}')
        return response['data']


if __name__ == '__main__':
    import sys
    client = WakewordClient(sys.argv[1]) if len(sys.argv) > 1 else WakewordClient()
    print(json.dumps(client.call('list'), ensure_ascii=False, indent=2))

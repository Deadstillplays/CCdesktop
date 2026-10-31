import os, sys, json, time, socket, base64, subprocess, urllib.request

def find_browser():
    candidates = [
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        os.path.expandvars(r"%LOCALAPPDATA%\BraveSoftware\Brave-Browser\Application\brave.exe"),
        os.path.expandvars(r"%LOCALAPPDATA%\Microsoft\Edge\Application\msedge.exe"),
        os.path.expandvars(r"%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe"),
    ]
    for c in candidates:
        if os.path.exists(c):
            return c
    return None

def get_websocket_frame(data_str):
    data_bytes = data_str.encode('utf-8')
    length = len(data_bytes)
    mask_key = os.urandom(4)
    if length <= 125:
        header = bytes([0x81, 0x80 | length]) + mask_key
    elif length <= 65535:
        header = bytes([0x81, 0x80 | 126, (length >> 8) & 0xFF, length & 0xFF]) + mask_key
    else:
        header = bytes([0x81, 0x80 | 127]) + length.to_bytes(8, 'big') + mask_key
    masked_data = bytes([b ^ mask_key[i % 4] for i, b in enumerate(data_bytes)])
    return header + masked_data

def read_websocket_frame(sock):
    try:
        header = sock.recv(2)
        if len(header) < 2:
            return None
        b1, b2 = header[0], header[1]
        is_masked = (b2 & 0x80) != 0
        payload_len = b2 & 0x7F
        if payload_len == 126:
            ext = sock.recv(2)
            payload_len = int.from_bytes(ext, 'big')
        elif payload_len == 127:
            ext = sock.recv(8)
            payload_len = int.from_bytes(ext, 'big')
        
        mask = sock.recv(4) if is_masked else None
        data = b""
        while len(data) < payload_len:
            chunk = sock.recv(payload_len - len(data))
            if not chunk:
                break
            data += chunk
        
        if is_masked and mask:
            data = bytes([b ^ mask[i % 4] for i, b in enumerate(data)])
        return data.decode('utf-8', errors='ignore')
    except Exception:
        return None

def main():
    browser = find_browser()
    if not browser:
        print(json.dumps({"error": "No Chromium browser (Brave, Edge, Chrome) found."}))
        sys.exit(1)

    profile_dir = os.path.expandvars(r"%APPDATA%\ClaudeDesktopRust\BrowserAuthProfile")
    os.makedirs(profile_dir, exist_ok=True)

    port = 9222
    url = "https://claude.ai/login"

    # Launch browser with remote debugging
    args = [
        browser,
        f"--remote-debugging-port={port}",
        f"--user-data-dir={profile_dir}",
        "--no-first-run",
        "--no-default-browser-check",
        url
    ]
    proc = subprocess.Popen(args)

    max_wait_seconds = 180
    start_time = time.time()
    found_key = None
    found_org = None

    while time.time() - start_time < max_wait_seconds:
        time.sleep(1.5)
        # Check if browser is still running
        if proc.poll() is not None:
            break

        try:
            with urllib.request.urlopen(f"http://localhost:{port}/json", timeout=2) as r:
                tabs = json.loads(r.read().decode())
                target_tab = next((t for t in tabs if 'claude.ai' in t.get('url', '')), None)
                if not target_tab and tabs:
                    target_tab = tabs[0]

                if not target_tab or 'webSocketDebuggerUrl' not in target_tab:
                    continue

                ws_url = target_tab['webSocketDebuggerUrl']
                host_port = ws_url.replace('ws://', '').split('/')[0]
                host, port_str = host_port.split(':')
                path = ws_url.replace(f'ws://{host_port}', '')

                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(2.5)
                s.connect((host, int(port_str)))
                key = base64.b64encode(os.urandom(16)).decode()
                req = (
                    f'GET {path} HTTP/1.1\r\n'
                    f'Host: {host_port}\r\n'
                    f'Upgrade: websocket\r\n'
                    f'Connection: Upgrade\r\n'
                    f'Sec-WebSocket-Key: {key}\r\n'
                    f'Sec-WebSocket-Version: 13\r\n\r\n'
                )
                s.sendall(req.encode())
                resp = s.recv(1024)
                if b'101' in resp:
                    cmd = json.dumps({'id': 1, 'method': 'Network.getCookies', 'params': {'urls': ['https://claude.ai']}})
                    s.sendall(get_websocket_frame(cmd))
                    reply = read_websocket_frame(s)
                    if reply:
                        parsed = json.loads(reply)
                        cookies = parsed.get('result', {}).get('cookies', [])
                        for c in cookies:
                            if c.get('name') == 'sessionKey':
                                val = c.get('value', '')
                                if val.startswith('sk-ant-sid'):
                                    found_key = val
                            elif c.get('name') == 'lastActiveOrg':
                                found_org = c.get('value')

                s.close()

                if found_key:
                    break
        except Exception:
            pass

    # Terminate the login browser window cleanly
    try:
        proc.terminate()
        time.sleep(0.5)
        proc.kill()
    except Exception:
        pass

    if found_key:
        print(json.dumps({
            "status": "success",
            "sessionKey": found_key,
            "lastActiveOrg": found_org
        }))
    else:
        print(json.dumps({"error": "Login timed out or window was closed before signing in."}))

if __name__ == "__main__":
    main()

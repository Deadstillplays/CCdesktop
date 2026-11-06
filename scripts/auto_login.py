import os
import sys
import json
import time
import socket
import base64
import shutil
import argparse
import subprocess
import urllib.request

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

def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

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

def parse_args():
    parser = argparse.ArgumentParser(description="Isolated Chromium Claude.ai Auto-Login")
    parser.add_argument("--profile-dir", dest="profile_dir", default=None, help="Path to dedicated profile directory")
    parser.add_argument("--account-id", dest="account_id", default=None, help="Target Claude account ID in app")
    parser.add_argument("--email", dest="email", default=None, help="Account email address to autofill")
    parser.add_argument("--url", dest="url", default="https://claude.ai/login", help="URL to navigate to")
    parser.add_argument("--clear", dest="clear", action="store_true", help="Clear profile cache before launch")
    
    # Backward compatibility with positional email argument
    parser.add_argument("positional_args", nargs="*", help="Optional positional email fallback")
    return parser.parse_args()

def main():
    args = parse_args()
    
    email_to_autofill = args.email
    if not email_to_autofill and args.positional_args:
        email_to_autofill = args.positional_args[0].strip()

    # Determine profile directory
    profile_dir = args.profile_dir
    if not profile_dir:
        # Default or fallback
        if email_to_autofill:
            safe_name = "".join(c if c.isalnum() or c in "._-" else "_" for c in email_to_autofill)
            profile_dir = os.path.expandvars(rf"%APPDATA%\ClaudeDesktopRust\Profiles\{safe_name}")
        elif args.account_id:
            safe_id = "".join(c if c.isalnum() or c in "._-" else "_" for c in args.account_id)
            profile_dir = os.path.expandvars(rf"%APPDATA%\ClaudeDesktopRust\Profiles\acc_{safe_id}")
        else:
            profile_dir = os.path.expandvars(r"%APPDATA%\ClaudeDesktopRust\Profiles\default")

    if args.clear and os.path.exists(profile_dir):
        try:
            shutil.rmtree(profile_dir, ignore_errors=True)
        except Exception:
            pass

    os.makedirs(profile_dir, exist_ok=True)

    browser = find_browser()
    if not browser:
        print(json.dumps({"status": "error", "error": "No Chromium browser (Brave, Edge, Chrome) found."}))
        sys.exit(1)

    port = find_free_port()
    url = args.url or "https://claude.ai/login"

    # Launch browser with remote debugging on isolated user data dir
    browser_args = [
        browser,
        f"--remote-debugging-port={port}",
        f"--user-data-dir={profile_dir}",
        "--no-first-run",
        "--no-default-browser-check",
        "--password-store=basic",
        url
    ]
    proc = subprocess.Popen(browser_args)

    max_wait_seconds = 300
    start_time = time.time()
    found_key = None
    found_org = None
    found_email = email_to_autofill
    found_name = None
    email_injected = False

    while time.time() - start_time < max_wait_seconds:
        time.sleep(1.2)
        if proc.poll() is not None:
            # User closed window before completing login
            break

        try:
            req = urllib.request.Request(f"http://127.0.0.1:{port}/json", headers={"User-Agent": "ClaudeDesktopRust/1.0"})
            with urllib.request.urlopen(req, timeout=2) as r:
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
                handshake = (
                    f'GET {path} HTTP/1.1\r\n'
                    f'Host: {host_port}\r\n'
                    f'Upgrade: websocket\r\n'
                    f'Connection: Upgrade\r\n'
                    f'Sec-WebSocket-Key: {key}\r\n'
                    f'Sec-WebSocket-Version: 13\r\n\r\n'
                )
                s.sendall(handshake.encode())
                resp = s.recv(1024)
                if b'101' in resp:
                    # Enable Network domain
                    enable_net = json.dumps({'id': 1, 'method': 'Network.enable', 'params': {}})
                    s.sendall(get_websocket_frame(enable_net))
                    read_websocket_frame(s)

                    # If email provided and not yet injected, try to inject into input field
                    if email_to_autofill and not email_injected:
                        expr = f"""
                        (function() {{
                            var inp = document.querySelector('input[type="email"]') || document.querySelector('input[name="email"]') || document.getElementById('email');
                            if (inp && (!inp.value || inp.value.length === 0)) {{
                                inp.focus();
                                inp.value = {json.dumps(email_to_autofill)};
                                inp.dispatchEvent(new Event('input', {{ bubbles: true }}));
                                inp.dispatchEvent(new Event('change', {{ bubbles: true }}));
                                return true;
                            }}
                            return false;
                        }})()
                        """
                        eval_cmd = json.dumps({'id': 10, 'method': 'Runtime.evaluate', 'params': {'expression': expr}})
                        s.sendall(get_websocket_frame(eval_cmd))
                        eval_reply = read_websocket_frame(s)
                        if eval_reply and '"value":true' in eval_reply:
                            email_injected = True

                    # Query cookies via Network.getCookies
                    cmd = json.dumps({'id': 20, 'method': 'Network.getCookies', 'params': {'urls': ['https://claude.ai', 'https://api.claude.ai']}})
                    s.sendall(get_websocket_frame(cmd))
                    reply = read_websocket_frame(s)
                    if reply:
                        try:
                            parsed = json.loads(reply)
                            cookies = parsed.get('result', {}).get('cookies', [])
                            for c in cookies:
                                if c.get('name') == 'sessionKey':
                                    val = c.get('value', '')
                                    if val.startswith('sk-ant-sid'):
                                        found_key = val
                                elif c.get('name') == 'lastActiveOrg':
                                    found_org = c.get('value')
                        except Exception:
                            pass

                    # Also query Storage.getCookies as backup
                    if not found_key:
                        cmd_storage = json.dumps({'id': 30, 'method': 'Storage.getCookies'})
                        s.sendall(get_websocket_frame(cmd_storage))
                        reply_storage = read_websocket_frame(s)
                        if reply_storage:
                            try:
                                parsed = json.loads(reply_storage)
                                cookies = parsed.get('result', {}).get('cookies', [])
                                for c in cookies:
                                    if c.get('name') == 'sessionKey':
                                        val = c.get('value', '')
                                        if val.startswith('sk-ant-sid'):
                                            found_key = val
                                    elif c.get('name') == 'lastActiveOrg':
                                        found_org = c.get('value')
                            except Exception:
                                pass

                    # If we found sessionKey, attempt to get real account info from page
                    if found_key:
                        info_expr = """
                        (async () => {
                            try {
                                const accRes = await window.fetch('/api/auth/current_account');
                                const acc = accRes.ok ? await accRes.json() : null;
                                const orgRes = await window.fetch('/api/organizations');
                                const orgs = orgRes.ok ? await orgRes.json() : [];
                                return {
                                    email: acc ? acc.email_address : null,
                                    name: acc ? (acc.full_name || acc.email_address) : null,
                                    orgUuid: (orgs && orgs.length > 0) ? orgs[0].uuid : null,
                                    orgName: (orgs && orgs.length > 0) ? orgs[0].name : null
                                };
                            } catch (e) {
                                return null;
                            }
                        })()
                        """
                        cmd_eval = json.dumps({'id': 40, 'method': 'Runtime.evaluate', 'params': {'expression': info_expr, 'awaitPromise': True, 'returnByValue': True}})
                        s.sendall(get_websocket_frame(cmd_eval))
                        info_reply = read_websocket_frame(s)
                        if info_reply:
                            try:
                                res_obj = json.loads(info_reply).get('result', {}).get('result', {}).get('value', {})
                                if res_obj:
                                    if res_obj.get('email'):
                                        found_email = res_obj['email']
                                    if res_obj.get('orgUuid'):
                                        found_org = res_obj['orgUuid']
                                    if res_obj.get('orgName'):
                                        found_name = f"{res_obj['orgName']}'s Organization"
                                    elif res_obj.get('name'):
                                        found_name = f"{res_obj['name']}'s Organization"
                            except Exception:
                                pass

                s.close()

                if found_key:
                    break
        except Exception:
            pass

    # Wait a moment for Chromium to flush cookie jar to SQLite database
    time.sleep(1.0)

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
            "targetAccountId": args.account_id,
            "orgUuid": found_org or "",
            "accountName": found_name or found_email or "Claude Account",
            "email": found_email or "",
            "profileDir": profile_dir
        }))
        sys.exit(0)
    else:
        print(json.dumps({
            "status": "error",
            "error": "Login timed out or window was closed before signing in.",
            "targetAccountId": args.account_id,
            "profileDir": profile_dir
        }))
        sys.exit(1)

if __name__ == "__main__":
    main()

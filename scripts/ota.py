#!/usr/bin/env python3
"""Discover, verify and stream signed ESP32-C6 firmware to a selected device."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import http.client
import json
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import sys
import threading
import time

MDNS_SERVICE_TYPES = ["_matter._tcp.local.", "_matterc._udp.local."]
OTA_HEADER_MAGIC = b"ICOT"
OTA_HEADER_LEN = 72
MAX_APP_SLOT_SIZE = 0x350000


class DiscoveredDevice:
    def __init__(self, name, hostname, ip, port, status):
        self.name = name
        self.hostname = hostname
        self.ip = ip
        self.port = port
        self.status = status

    def display_line(self):
        return f"{self.name} ({self.hostname} {self.ip}:{self.port})"


def probe_status(ip, device_type, port=8080, timeout=1.5):
    """Return status only when the device asserts the exact expected type."""
    conn = None
    try:
        conn = http.client.HTTPConnection(ip, port, timeout=timeout)
        conn.request("GET", "/api/status")
        response = conn.getresponse()
        if response.status != 200:
            return None
        status = json.loads(response.read().decode())
        if isinstance(status, dict) and status.get("device_type") == device_type:
            return status
    except (OSError, ValueError, http.client.HTTPException):
        pass
    finally:
        if conn is not None:
            conn.close()
    return None


def discover_devices(device_type, timeout=3.5, port=8080):
    """Resolve Matter mDNS services and verify each advertised HTTP device."""
    try:
        import zeroconf
    except ImportError as error:
        raise RuntimeError("mDNS discovery requires the optional zeroconf package") from error

    class Listener:
        def __init__(self, zc):
            self.zc = zc
            self.addresses = set()
            self.lock = threading.Lock()

        def add_service(self, zc, service_type, name):
            self.update_service(zc, service_type, name)

        def update_service(self, zc, service_type, name):
            info = self.zc.get_service_info(service_type, name, timeout=1500)
            if info:
                for address in info.parsed_addresses():
                    if re.fullmatch(r"\d{1,3}(?:\.\d{1,3}){3}", address):
                        with self.lock:
                            self.addresses.add((address, (info.server or "").rstrip(".")))

        def remove_service(self, zc, service_type, name):
            pass

    zc = zeroconf.Zeroconf()
    try:
        listener = Listener(zc)
        browser = zeroconf.ServiceBrowser(zc, MDNS_SERVICE_TYPES, listener)
        try:
            time.sleep(timeout)
        finally:
            browser.cancel()
    finally:
        zc.close()
    with listener.lock:
        candidates = list(listener.addresses)
    def verify(candidate):
        ip, hostname = candidate
        status = probe_status(ip, device_type, port=port)
        if status is None:
            return None
        return DiscoveredDevice(status.get("name") or device_type, hostname, ip, port, status)

    with ThreadPoolExecutor(max_workers=20) as pool:
        devices = [device for device in pool.map(verify, candidates) if device]
    return sorted(devices, key=lambda device: (device.name, device.ip))


def parse_direct_target(target, default_port=8080):
    match = re.fullmatch(r"(\d{1,3}(?:\.\d{1,3}){3})(?::(\d{1,5}))?", target)
    if match:
        port = int(match.group(2)) if match.group(2) else default_port
        if not 1 <= port <= 65535:
            raise ValueError("Target port must be 1..65535")
        return match.group(1), port
    return None


def match_target(target, devices):
    query = target.casefold().strip()
    return [device for device in devices if query in (device.ip.casefold(), device.hostname.casefold())
            or query in device.name.casefold() or query.replace(":", "") == str(device.status.get("mac", "")).casefold().replace(":", "")]


def resolve_targets(target, device_type, port, timeout):
    direct = parse_direct_target(target, port) if target else None
    if direct:
        ip, target_port = direct
        status = probe_status(ip, device_type, port=target_port)
        if status is None:
            raise ValueError(f"Target {ip}:{target_port} did not report device_type={device_type!r}")
        return [DiscoveredDevice(status.get("name") or device_type, "", ip, target_port, status)]

    if target:
        hostname = target.rstrip(".")
        try:
            ip = socket.gethostbyname(hostname)
        except socket.gaierror:
            pass
        else:
            status = probe_status(ip, device_type, port=port)
            if status is None:
                raise ValueError(f"Target {target!r} did not report device_type={device_type!r}")
            return [DiscoveredDevice(status.get("name") or device_type, hostname, ip, port, status)]

    discovered = discover_devices(device_type, timeout=timeout, port=port)
    matches = match_target(target, discovered) if target else discovered
    if not matches:
        raise ValueError(f"No devices reporting device_type={device_type!r} found")
    if len(matches) == 1:
        return matches
    for index, device in enumerate(matches, 1):
        print(f"[{index}] {device.display_line()}")
    choice = input(f"Select 1-{len(matches)} or A for all: ").strip()
    if choice.upper() == "A":
        return matches
    if choice.isdigit() and 1 <= int(choice) <= len(matches):
        return [matches[int(choice) - 1]]
    raise ValueError("Invalid device selection")


def sign_firmware(binary_data, key_path):
    try:
        from cryptography.hazmat.primitives import serialization
        from cryptography.hazmat.primitives.asymmetric import ed25519
    except ImportError as error:
        raise RuntimeError("Signing requires the optional cryptography package") from error
    private_key = serialization.load_pem_private_key(key_path.read_bytes(), password=None)
    if not isinstance(private_key, ed25519.Ed25519PrivateKey):
        raise ValueError("Signing key must be Ed25519")
    return private_key.sign(binary_data)


def build_ota_header(binary_len, signature):
    if len(signature) != 64 or not 0 <= binary_len <= 0xffffffff:
        raise ValueError("Invalid OTA length or Ed25519 signature")
    return OTA_HEADER_MAGIC + struct.pack("<I", binary_len) + signature


def convert_elf_to_bin(elf_path):
    import tempfile
    fd, path = tempfile.mkstemp(prefix="edf_ota_", suffix=".bin")
    os.close(fd)
    output = Path(path)
    try:
        subprocess.run(["mise", "exec", "--", "espflash", "save-image", "--chip", "esp32c6",
                        "--flash-size", "8mb", str(elf_path), str(output)], check=True)
        return output
    except Exception:
        output.unlink(missing_ok=True)
        raise


def push_ota_to_device(device, header, binary_path, binary_len, http_timeout=60.0):
    conn = http.client.HTTPConnection(device.ip, device.port, timeout=http_timeout)
    try:
        conn.putrequest("POST", "/api/v1/ota/flash")
        conn.putheader("Content-Length", str(len(header) + binary_len))
        conn.putheader("Content-Type", "application/octet-stream")
        conn.endheaders()
        conn.send(header)
        with binary_path.open("rb") as binary:
            while True:
                chunk = binary.read(4096)
                if not chunk:
                    break
                conn.send(chunk)
        response = conn.getresponse()
        body = response.read().decode(errors="replace")
        if response.status != 200:
            print(f"OTA rejected by {device.ip}: HTTP {response.status} {body}", file=sys.stderr)
            return False
        print(f"Updated {device.display_line()}: {body}")
        return True
    except (OSError, http.client.HTTPException) as error:
        print(f"OTA transfer failed for {device.ip}: {error}", file=sys.stderr)
        return False
    finally:
        conn.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description="Signed ESP32-C6 OTA firmware updater")
    parser.add_argument("firmware", help="ELF or flat firmware image")
    parser.add_argument("--device-type", required=True, help="Exact /api/status device_type expected")
    parser.add_argument("--key", "-k", required=True, type=Path, help="Ed25519 PEM signing key")
    parser.add_argument("--target", "-t", help="IP, hostname, MAC or device name")
    parser.add_argument("--port", "-p", type=int, default=8080)
    parser.add_argument("--timeout", type=float, default=3.5)
    parser.add_argument("--http-timeout", type=float, default=60.0)
    args = parser.parse_args(argv)
    if not args.device_type.strip():
        parser.error("--device-type must be nonempty")
    binary_path = Path(args.firmware).resolve()
    if not binary_path.is_file():
        parser.error(f"Firmware not found: {binary_path}")
    try:
        # Resolve and verify every target before opening the private key or signing firmware.
        targets = resolve_targets(args.target, args.device_type, args.port, args.timeout)
        for target in targets:
            status = probe_status(target.ip, args.device_type, port=target.port)
            if status is None:
                raise ValueError(f"Target {target.ip}:{target.port} did not report device_type={args.device_type!r}")
            target.status = status
        with binary_path.open("rb") as binary:
            magic = binary.read(4)
        if magic == b"\x7fELF":
            binary_path = convert_elf_to_bin(binary_path)
        binary_len = binary_path.stat().st_size
        if not 0 < binary_len <= MAX_APP_SLOT_SIZE:
            raise ValueError("Firmware image exceeds OTA slot or is empty")
        signature = sign_firmware(binary_path.read_bytes(), args.key.expanduser())
        header = build_ota_header(binary_len, signature)
    except (ValueError, OSError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Error: {error}\n")
    results = []
    for device in targets:
        if probe_status(device.ip, args.device_type, port=device.port) is None:
            print(f"Skipping {device.ip}: device_type changed or status unavailable", file=sys.stderr)
            results.append(False)
            continue
        results.append(push_ota_to_device(device, header, binary_path, binary_len, args.http_timeout))
    if not all(results):
        parser.exit(1, "One or more updates failed\n")


if __name__ == "__main__":
    main()

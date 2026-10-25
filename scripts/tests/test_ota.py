import importlib.util
from pathlib import Path
import struct
import socket
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("edf_ota", Path(__file__).resolve().parents[1] / "ota.py")
ota = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ota)

class Response:
    status = 200
    def __init__(self, device_type): self.device_type = device_type
    def read(self):
        import json
        return json.dumps({"device_type": self.device_type}).encode()

class Connection:
    def __init__(self, device_type): self.response = Response(device_type)
    def request(self, *_): pass
    def getresponse(self): return self.response
    def close(self): pass

class OtaTests(unittest.TestCase):
    def test_header_wire_format(self):
        signature = bytes(range(64))
        self.assertEqual(ota.build_ota_header(0x12345678, signature), b"ICOT" + struct.pack("<I", 0x12345678) + signature)

    def test_exact_type_is_accepted(self):
        with patch.object(ota.http.client, "HTTPConnection", return_value=Connection("hvac_bridge")):
            self.assertIsNotNone(ota.probe_status("127.0.0.1", "hvac_bridge"))

    def test_missing_type_is_rejected(self):
        with patch.object(ota.http.client, "HTTPConnection", return_value=Connection(None)):
            self.assertIsNone(ota.probe_status("127.0.0.1", "hvac_bridge"))

    def test_mismatched_type_is_rejected(self):
        with patch.object(ota.http.client, "HTTPConnection", return_value=Connection("intercom_bridge")):
            self.assertIsNone(ota.probe_status("127.0.0.1", "hvac_bridge"))

    def test_direct_target_rejected_before_signing(self):
        with patch.object(ota, "probe_status", return_value=None), patch.object(ota, "sign_firmware") as sign:
            with self.assertRaises(SystemExit):
                ota.main(["--device-type", "hvac_bridge", "--key", "/absent/key", "--target", "127.0.0.1", __file__])
            sign.assert_not_called()

    def test_discovered_target_rejected_before_signing(self):
        device = ota.DiscoveredDevice("fixture", "fixture.local", "127.0.0.2", 8080, {"device_type": "hvac_bridge"})
        with patch.object(ota, "discover_devices", return_value=[device]), patch.object(ota, "probe_status", return_value=None), patch.object(ota, "sign_firmware") as sign:
            with self.assertRaises(SystemExit):
                ota.main(["--device-type", "hvac_bridge", "--key", "/absent/key", __file__])
            sign.assert_not_called()

    def test_matching_direct_target_reaches_mock_upload(self):
        status = {"device_type": "hvac_bridge"}
        with patch.object(ota, "probe_status", return_value=status), patch.object(ota, "sign_firmware", return_value=bytes(64)) as sign, patch.object(ota, "push_ota_to_device", return_value=True) as upload:
            ota.main(["--device-type", "hvac_bridge", "--key", "/absent/key", "--target", "127.0.0.1", __file__])
        sign.assert_called_once()
        upload.assert_called_once()

    def test_explicit_hostname_resolves_without_mdns(self):
        with patch.object(ota.socket, "gethostbyname", return_value="192.0.2.42"), patch.object(ota, "probe_status", return_value={"device_type": "hvac_bridge"}), patch.object(ota, "discover_devices") as discover:
            targets = ota.resolve_targets("bridge.local", "hvac_bridge", 8080, 3.5)
        self.assertEqual(targets[0].ip, "192.0.2.42")
        discover.assert_not_called()

    def test_explicit_hostname_wrong_type_rejected_before_signing(self):
        with patch.object(ota.socket, "gethostbyname", return_value="192.0.2.43"), patch.object(ota, "probe_status", return_value=None), patch.object(ota, "discover_devices") as discover, patch.object(ota, "sign_firmware") as sign:
            with self.assertRaises(SystemExit):
                ota.main(["--device-type", "hvac_bridge", "--key", "/absent/key", "--target", "bridge.local", __file__])
        discover.assert_not_called()
        sign.assert_not_called()

    def test_unresolved_device_name_falls_back_to_discovery(self):
        device = ota.DiscoveredDevice("Front Door", "front.local", "192.0.2.44", 8080, {"device_type": "intercom_bridge"})
        with patch.object(ota.socket, "gethostbyname", side_effect=socket.gaierror), patch.object(ota, "discover_devices", return_value=[device]) as discover:
            targets = ota.resolve_targets("Front Door", "intercom_bridge", 8080, 3.5)
        self.assertEqual(targets, [device])
        discover.assert_called_once()

if __name__ == "__main__": unittest.main()

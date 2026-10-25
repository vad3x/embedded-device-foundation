import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("edf_monitor", Path(__file__).resolve().parents[1] / "monitor.py")
monitor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(monitor)

class MonitorTests(unittest.TestCase):
    def test_default_does_not_reset(self):
        args = monitor.parse_args([])
        with patch.object(monitor.fcntl, "ioctl") as ioctl:
            monitor.maybe_reset(123, args.reset)
        ioctl.assert_not_called()

    def test_reset_option_toggles_control_lines(self):
        args = monitor.parse_args(["--reset"])
        with patch.object(monitor.fcntl, "ioctl") as ioctl, patch.object(monitor.time, "sleep"):
            monitor.maybe_reset(123, args.reset)
        self.assertEqual(ioctl.call_count, 3)

    def test_interrupt_closes_open_serial_descriptor(self):
        with patch.object(monitor, "open_and_configure_port", return_value=17), patch.object(monitor.select, "select", side_effect=KeyboardInterrupt), patch.object(monitor.os, "close") as close:
            monitor.main(["--port", "/fake/serial"])
        close.assert_called_once_with(17)

    def test_reset_error_closes_descriptor_before_retry(self):
        with patch.object(monitor, "open_and_configure_port", return_value=18), patch.object(monitor, "maybe_reset", side_effect=OSError("reset failed")), patch.object(monitor.time, "sleep", side_effect=KeyboardInterrupt), patch.object(monitor.os, "close") as close:
            monitor.main(["--port", "/fake/serial", "--reset"])
        close.assert_called_once_with(18)

    def test_failed_open_never_closes_uninitialized_descriptor(self):
        with patch.object(monitor, "open_and_configure_port", side_effect=OSError("unavailable")), patch.object(monitor.time, "sleep", side_effect=KeyboardInterrupt), patch.object(monitor.os, "close") as close:
            monitor.main(["--port", "/fake/serial"])
        close.assert_not_called()

    def test_read_error_closes_descriptor_before_retry(self):
        with patch.object(monitor, "open_and_configure_port", return_value=19), patch.object(monitor.select, "select", return_value=([19], [], [])), patch.object(monitor.os, "read", side_effect=OSError("read failed")), patch.object(monitor.time, "sleep", side_effect=KeyboardInterrupt), patch.object(monitor.os, "close") as close:
            monitor.main(["--port", "/fake/serial"])
        close.assert_called_once_with(19)

if __name__ == "__main__": unittest.main()

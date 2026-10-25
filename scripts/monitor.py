#!/usr/bin/env python3
"""ESP32-C6 UART serial monitor; DTR/RTS reset is explicitly opt-in."""

import argparse
import fcntl
import glob
import os
import select
import struct
import sys
import termios
import time


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description="ESP32-C6 serial monitor")
    parser.add_argument("--port", "-p", help="Serial port (auto detected if omitted)")
    parser.add_argument("--baud", "-b", type=int, default=115200)
    parser.add_argument("--reset", action="store_true", help="Pulse DTR/RTS to reset chip on connect")
    parser.add_argument("--log-file", "-l", help="Append decoded output to this file")
    return parser.parse_args(argv)


def find_serial_port():
    ports = sorted(glob.glob("/dev/cu.usbserial*") + glob.glob("/dev/cu.usbmodem*"))
    return ports[0] if ports else None


def reset_chip(fd):
    fcntl.ioctl(fd, termios.TIOCMBIC, struct.pack("I", termios.TIOCM_DTR))
    fcntl.ioctl(fd, termios.TIOCMBIS, struct.pack("I", termios.TIOCM_RTS))
    time.sleep(0.1)
    fcntl.ioctl(fd, termios.TIOCMBIC, struct.pack("I", termios.TIOCM_RTS))
    time.sleep(0.05)


def maybe_reset(fd, enabled):
    if enabled:
        reset_chip(fd)


def open_and_configure_port(port, baud):
    baud_const = getattr(termios, f"B{baud}", None)
    if baud_const is None:
        raise ValueError(f"Unsupported baud rate: {baud}")
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        attrs = termios.tcgetattr(fd)
        attrs[4] = baud_const
        attrs[5] = baud_const
        attrs[3] &= ~(termios.ICANON | termios.ECHO)
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
        return fd
    except BaseException:
        os.close(fd)
        raise


def main(argv=None):
    args = parse_args(argv)
    port = args.port or find_serial_port()
    if port is None:
        raise SystemExit("No serial port found; specify --port")
    log = open(args.log_file, "a", buffering=1) if args.log_file else None
    try:
        while True:
            fd = None
            try:
                fd = open_and_configure_port(port, args.baud)
                maybe_reset(fd, args.reset)
                print(f"Monitoring {port} at {args.baud} baud", flush=True)
                buffer = ""
                while True:
                    readable, _, _ = select.select([fd], [], [], 1.0)
                    if not readable:
                        continue
                    chunk = os.read(fd, 1024)
                    if not chunk:
                        break
                    buffer += chunk.decode("utf-8", errors="replace")
                    while "\n" in buffer:
                        line, buffer = buffer.split("\n", 1)
                        line = line.rstrip("\r")
                        print(line, flush=True)
                        if log:
                            log.write(line + "\n")
            except KeyboardInterrupt:
                return
            except (OSError, ValueError) as error:
                print(f"Serial port unavailable: {error}", file=sys.stderr)
            finally:
                if fd is not None:
                    os.close(fd)
            if not args.port:
                port = find_serial_port() or port
            time.sleep(0.5)
    except KeyboardInterrupt:
        return
    finally:
        if log:
            log.close()


if __name__ == "__main__":
    main()

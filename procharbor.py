#!/usr/bin/env python3
"""ProcHarbor — a native GTK4 system monitor for Linux."""

from __future__ import annotations

import configparser
import fcntl
import glob
import os
import pwd
import re
import shlex
import signal
import socket
import struct
import subprocess
import sys
import threading
import time
import warnings
from string import Template

warnings.filterwarnings("ignore", category=DeprecationWarning)

try:
    import gi

    gi.require_version("Gtk", "4.0")
    gi.require_version("Gdk", "4.0")
    from gi.repository import Gdk, GLib, Gtk, Pango
    import cairo
except (ImportError, ValueError) as exc:
    print("This app needs GTK4 and PyGObject (python3-gi, gir1.2-gtk-4.0).", file=sys.stderr)
    print(exc, file=sys.stderr)
    sys.exit(1)

HISTORY_LEN = 120
CLK_TCK = os.sysconf("SC_CLK_TCK")
PAGE_SIZE = os.sysconf("SC_PAGE_SIZE")
MY_UID = os.getuid()
HERE = os.path.dirname(os.path.abspath(__file__))
APP_ID = "io.github.shripad-jyothinath.procharbor"
__version__ = "1.3.0"

STATE_LABELS = {
    "R": "Running", "S": "Sleeping", "D": "Disk sleep", "I": "Idle",
    "T": "Suspended", "t": "Suspended", "Z": "Zombie", "X": "Dead",
    "x": "Dead", "W": "Paging", "P": "Parked",
}

ENGINE_NAMES = {
    "render": "3D", "video": "Video", "video-enhance": "Video", "copy": "Copy",
    "vecs": "Compute", "ccs": "Compute", "vcs": "Video", "bcs": "Copy", "rcs": "3D",
}

TABS = [
    ("processes", "view-grid-symbolic", "Processes"),
    ("performance", "utilities-system-monitor-symbolic", "Performance"),
    ("apps", "document-open-recent-symbolic", "App history"),
    ("startup", "media-playback-start-symbolic", "Startup"),
    ("users", "system-users-symbolic", "Users"),
    ("details", "view-list-symbolic", "Details"),
    ("services", "preferences-system-symbolic", "Services"),
]

PERF_ITEMS = [
    ("cpu", "CPU", "#4cc2ff"),
    ("memory", "Memory", "#a68bff"),
    ("disk", "Disk", "#5ad48a"),
    ("wifi", "Wi-Fi", "#f7b955"),
    ("gpu", "GPU", "#ff7676"),
]

TEXT_KEYS = {
    "name", "status", "user", "unit", "type", "scope", "command", "description",
    "session", "state", "cmdline", "active", "startup",
}

THEMES = {
    "dark": {
        "bg": "#1f1f1f", "side": "#1a1a1a", "panel": "#272727", "panel2": "#2d2d2d",
        "border": "#3a3a3a", "border_soft": "#313131", "text": "#ececec",
        "dim": "#a2a2a2", "faint": "#787878", "accent": "#4cc2ff",
        "accent_soft": "rgba(76,194,255,0.13)", "danger": "#ff7676",
        "green": "#5ad48a", "amber": "#f7b955", "orange": "#ff9f43", "purple": "#a68bff",
        "sel": "#123a52", "hover": "#2b2b2b",
        "chart_grid": "#3a3a3a", "chart_text": "#787878",
    },
    "light": {
        "bg": "#f4f4f4", "side": "#eaeaea", "panel": "#ffffff", "panel2": "#f7f7f7",
        "border": "#d2d2d2", "border_soft": "#e3e3e3", "text": "#1b1b1b",
        "dim": "#5d5d5d", "faint": "#8a8a8a", "accent": "#0f7ac2",
        "accent_soft": "rgba(15,122,194,0.12)", "danger": "#c22f2f",
        "green": "#1a8a4a", "amber": "#a86a00", "orange": "#b45309", "purple": "#6d4fd6",
        "sel": "#cfe8fb", "hover": "#e8e8e8",
        "chart_grid": "#d2d2d2", "chart_text": "#8a8a8a",
    },
}

CSS_TEMPLATE = Template("""
window {
  background-color: $bg;
  color: $text;
  font-family: "Inter", "Cantarell", "Noto Sans", sans-serif;
  font-size: 13px;
}
headerbar {
  background-color: $side;
  border-bottom: 1px solid $border_soft;
  box-shadow: none;
  min-height: 46px;
}
headerbar button {
  background-color: $panel;
  border: 1px solid $border;
  border-radius: 6px;
  padding: 5px 12px;
  color: $text;
}
headerbar button:hover { background-color: $panel2; }
headerbar button.danger { color: $danger; }
searchentry {
  background-color: $panel;
  border: 1px solid $border;
  border-radius: 6px;
  padding: 2px 8px;
  color: $text;
}
searchentry:focus-within { border-color: $accent; }
searchentry entry { background: transparent; color: $text; }
entry {
  background-color: $panel;
  color: $text;
  border: 1px solid $border;
  border-radius: 6px;
  padding: 4px 9px;
}
entry:focus-within { border-color: $accent; }
.sidebar { background-color: $side; border-right: 1px solid $border_soft; }
.brand { padding: 14px 14px 10px 14px; }
.brand-title { font-size: 15px; font-weight: 700; color: $text; }
.nav { background: transparent; }
.nav row { border-radius: 6px; padding: 1px; margin: 1px 6px; }
.nav row:hover { background-color: $hover; }
.nav row:selected { background-color: $accent_soft; }
.nav row:selected label { color: $text; }
.nav row label { color: $text; }
.nav image { color: $dim; }
.nav row:selected image { color: $accent; }
.summary {
  color: $dim;
  font-size: 12px;
  padding: 7px 16px;
  border-bottom: 1px solid $border_soft;
}
.status { color: $dim; font-size: 12px; padding: 2px 14px 8px 14px; }
treeview.view {
  background-color: $bg;
  color: $text;
}
treeview.view:selected {
  background-color: $sel;
  color: $text;
}
treeview.view:hover { background-color: $hover; }
treeview header button {
  background-color: $bg;
  color: $dim;
  border: none;
  border-bottom: 1px solid $border;
  border-radius: 0;
  padding: 5px 8px;
  font-weight: 500;
  font-size: 12px;
}
treeview header button:hover { color: $text; }
treeview.view expander { color: $dim; }
scrollbar { background: transparent; }
scrollbar slider {
  background-color: $border;
  border-radius: 6px;
  min-width: 8px;
  min-height: 8px;
}
scrollbar slider:hover { background-color: $faint; }
.perf-list { background: transparent; }
.perf-list row { border-radius: 6px; padding: 1px; margin: 1px 6px; }
.perf-list row:hover { background-color: $hover; }
.perf-list row:selected { background-color: $accent_soft; }
.perf-name { font-weight: 600; color: $text; }
.perf-title { font-size: 23px; font-weight: 600; color: $text; }
.muted { color: $dim; }
.faint { color: $faint; }
.card {
  background-color: $panel;
  border: 1px solid $border;
  border-radius: 8px;
  padding: 12px;
}
.stat-label { color: $dim; font-size: 12px; }
.stat-value { font-size: 18px; font-weight: 600; color: $text; }
.stat-sub { color: $faint; font-size: 11px; }
.section-title { font-size: 13px; font-weight: 600; color: $text; }
.core-cell {
  background-color: $panel;
  border: 1px solid $border;
  border-radius: 6px;
  padding: 6px;
}
.core-label { font-size: 11px; color: $dim; }
.extra-title { font-size: 13px; font-weight: 600; color: $text; }
.vol { font-size: 12px; color: $text; }
.vol-dim { font-size: 12px; color: $dim; }
progressbar trough {
  background-color: $panel2;
  border-radius: 3px;
  min-height: 6px;
}
progressbar progress {
  background-color: $accent;
  border-radius: 3px;
  min-height: 6px;
}
.composition { background: transparent; }
button.flat-theme {
  background: transparent;
  border: none;
  color: $dim;
  padding: 4px 8px;
}
button.flat-theme:hover { background-color: $hover; color: $text; }
""")


def read_text(path: str, default: str = "") -> str:
    try:
        with open(path, "r", errors="replace") as fh:
            return fh.read()
    except OSError:
        return default


def read_int(path: str, default=None):
    try:
        return int(read_text(path).strip())
    except (ValueError, TypeError):
        return default


def run(cmd, timeout=5.0):
    try:
        return subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout,
            env={**os.environ, "LC_ALL": "C", "LANG": "C"},
        )
    except (OSError, subprocess.SubprocessError):
        return None


def json_cmd(cmd, timeout=8.0):
    p = run(cmd, timeout)
    if p and p.returncode == 0 and p.stdout.strip():
        try:
            import json
            return json.loads(p.stdout)
        except ValueError:
            return None
    return None


def cpu_model() -> str:
    for line in read_text("/proc/cpuinfo").splitlines():
        if line.startswith("model name"):
            return line.split(":", 1)[1].strip()
    return "CPU"


def physical_cores() -> int:
    ids = set()
    for d in glob.glob("/sys/devices/system/cpu/cpu[0-9]*/topology"):
        ids.add((read_int(d + "/physical_package_id", 0), read_int(d + "/core_id", 0)))
    return len(ids) or (os.cpu_count() or 1)


def lscpu_limits():
    p = run(["lscpu"], 4)
    mn = mx = None
    if p and p.returncode == 0:
        for line in p.stdout.splitlines():
            if ":" not in line:
                continue
            k, v = [x.strip() for x in line.split(":", 1)]
            try:
                if k == "CPU max MHz":
                    mx = float(v)
                elif k == "CPU min MHz":
                    mn = float(v)
            except ValueError:
                pass
    return mn, mx


def boot_time() -> float:
    for line in read_text("/proc/stat").splitlines():
        if line.startswith("btime "):
            try:
                return float(line.split()[1])
            except (ValueError, IndexError):
                break
    return time.time()


def iface_ipv4(ifname: str):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        packed = struct.pack("256s", ifname[:15].encode())
        return socket.inet_ntoa(fcntl.ioctl(s.fileno(), 0x8915, packed)[20:24])
    except OSError:
        return None
    finally:
        s.close()


def iface_ipv6(ifname: str):
    out = []
    for line in read_text("/proc/net/if_inet6").splitlines():
        parts = line.split()
        if len(parts) >= 6 and parts[5] == ifname:
            raw = parts[0]
            try:
                out.append(socket.inet_ntop(socket.AF_INET6, bytes.fromhex(raw)))
            except (ValueError, OSError):
                pass
    return out


def freq_to_channel(ghz: float):
    if not ghz:
        return None
    mhz = ghz * 1000.0
    if 2412 <= mhz <= 2484:
        return 14 if mhz == 2484 else int((mhz - 2407) / 5)
    if 5000 <= mhz <= 5925:
        return int((mhz - 5000) / 5)
    return None


class Sampler:
    def __init__(self):
        self.lock = threading.RLock()
        self.stop = threading.Event()
        self.start_ts = time.time()
        self.ncpu = os.cpu_count() or 1
        self.cpu_model = cpu_model()
        self.phys_cores = physical_cores()
        self.sockets = len({
            read_int(path, 0)
            for path in glob.glob("/sys/devices/system/cpu/cpu[0-9]*/topology/physical_package_id")
        })
        self.cpu_flags = set()
        for line in read_text("/proc/cpuinfo").splitlines():
            if line.startswith("flags"):
                self.cpu_flags = set(line.split(":", 1)[1].split())
                break
        self.cpu_min_mhz, self.cpu_max_mhz = lscpu_limits()
        self.boot_time = boot_time()
        self.gpu_name, self.gpu_card = self._find_gpu()
        self.disk_models = {}
        self.hwmon = {"cpu": None, "nvme": None, "wifi": None}

        self._uid_names = {}
        self._meta = {}
        self._prev_proc = {}
        self._pid_app = {}
        self._fdcount = {}
        self._gpu_prev = {}
        self._gpu_proc = {}
        self._gpu_ts = 0.0
        self._gpu_total = 0.0
        self._prev_net = {}
        self._net_proc = {}
        self._net_ts = 0.0
        self._app_totals = {}
        self._sys_prev_cpu = None
        self._disk_prev = {}
        self._net_if_prev = {}
        self._scan_count = 0
        self._proc_samples = 0
        self._proc_list = []
        self._totals = {"processes": 0, "threads": 0, "handles": 0}
        self._system = {}
        self._proc_ts = 0.0
        self._sys_ts = 0.0
        self._hist = {
            "cpu": [], "cores": [], "mem": [], "disk_r": [], "disk_w": [],
            "net_rx": [], "net_tx": [], "gpu": [],
        }
        self._services = {"ts": 0.0, "data": []}
        self._startup = {"ts": 0.0, "data": []}
        self._sessions = {"ts": 0.0, "data": []}
        self._host = {
            "hostname": socket.gethostname(),
            "kernel": os.uname().release,
            "os": self._os_name(),
            "boot_time": self.boot_time,
        }

    @staticmethod
    def _os_name() -> str:
        for line in read_text("/etc/os-release").splitlines():
            if line.startswith("PRETTY_NAME="):
                return line.split("=", 1)[1].strip().strip('"')
        return "Linux"

    def _find_gpu(self):
        for card in sorted(glob.glob("/sys/class/drm/card[0-9]*")):
            device_path = os.path.join(card, "device")
            if not os.path.isdir(device_path):
                continue
            pci = os.path.basename(os.path.realpath(device_path))
            name = None
            p = run(["lspci", "-mm", "-s", pci], 3)
            if p and p.returncode == 0:
                fields = re.findall(r'"([^"]*)"', p.stdout)
                if len(fields) >= 3 and fields[2]:
                    name = fields[2]
            if not name:
                vendor = read_text(os.path.join(device_path, "vendor")).strip().lower()
                if vendor == "0x8086":
                    name = "Intel Graphics"
                elif vendor == "0x1002":
                    name = "AMD Radeon Graphics"
                elif vendor == "0x10de":
                    name = "NVIDIA Graphics"
                else:
                    name = "GPU"
            return name, card
        return None, None

    def _uid_name(self, uid: int) -> str:
        if uid not in self._uid_names:
            try:
                self._uid_names[uid] = pwd.getpwuid(uid).pw_name
            except KeyError:
                self._uid_names[uid] = str(uid)
        return self._uid_names[uid]

    def _refresh_hwmon(self):
        for h in glob.glob("/sys/class/hwmon/hwmon*"):
            name = read_text(os.path.join(h, "name")).strip()
            if name == "coretemp":
                self.hwmon["cpu"] = h
            elif name == "nvme":
                self.hwmon["nvme"] = h
            elif name.startswith("iwlwifi"):
                self.hwmon["wifi"] = h

    def _temp(self, kind: str):
        path = self.hwmon.get(kind)
        if path and os.path.isdir(path):
            if kind == "cpu":
                best = None
                for inp in glob.glob(path + "/temp*_input"):
                    label = read_text(inp[:-6] + "_label").strip()
                    val = read_int(inp)
                    if val is None:
                        continue
                    if label.startswith("Package"):
                        return val / 1000.0
                    if best is None:
                        best = val
                if best is not None:
                    return best / 1000.0
            else:
                val = read_int(path + "/temp1_input")
                if val is not None:
                    return val / 1000.0
        self._refresh_hwmon()
        return None

    def _disk_model(self, name: str) -> str:
        if name in self.disk_models:
            return self.disk_models[name]
        model = read_text(f"/sys/block/{name}/device/model").strip()
        if not model and name.startswith("nvme"):
            model = read_text(f"/sys/class/nvme/{name[:-2]}/model").strip()
        self.disk_models[name] = model or name
        return self.disk_models[name]

    def start(self):
        self._scan_gpu(time.time())
        threading.Thread(target=self._system_loop, daemon=True, name="sys-sampler").start()
        threading.Thread(target=self._proc_loop, daemon=True, name="proc-sampler").start()

    def _system_loop(self):
        self._refresh_hwmon()
        while not self.stop.is_set():
            t0 = time.time()
            try:
                self.sample_system()
            except Exception as exc:
                print("system sample error:", exc, file=sys.stderr)
            self.stop.wait(max(0.05, 1.0 - (time.time() - t0)))

    def _proc_loop(self):
        while not self.stop.is_set():
            t0 = time.time()
            try:
                self.sample_processes()
            except Exception as exc:
                print("process sample error:", exc, file=sys.stderr)
            self.stop.wait(max(0.05, 2.0 - (time.time() - t0)))

    def _read_cpu_times(self):
        data = {}
        for line in read_text("/proc/stat").splitlines():
            if not line.startswith("cpu"):
                continue
            parts = line.split()
            if len(parts) < 9:
                continue
            vals = [int(x) for x in parts[1:9]]
            data[parts[0]] = (sum(vals), vals[3] + vals[4])
        return data

    def sample_system(self):
        now = time.time()
        cpu_now = self._read_cpu_times()
        prev = self._sys_prev_cpu
        per_core = [0.0] * self.ncpu
        util = 0.0
        if prev:
            for i in range(self.ncpu):
                t0, i0 = prev.get(f"cpu{i}", (0, 0))
                t1, i1 = cpu_now.get(f"cpu{i}", (0, 0))
                dt = t1 - t0
                per_core[i] = max(0.0, min(100.0, 100.0 * (1.0 - (i1 - i0) / dt))) if dt > 0 else 0.0
            t0, i0 = prev.get("cpu", (0, 0))
            t1, i1 = cpu_now.get("cpu", (0, 0))
            dt = t1 - t0
            util = max(0.0, min(100.0, 100.0 * (1.0 - (i1 - i0) / dt))) if dt > 0 else 0.0
        self._sys_prev_cpu = cpu_now

        freqs = []
        for i in range(self.ncpu):
            v = read_int(f"/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq", 0) or 0
            freqs.append(v / 1000.0)
        freq_avg = sum(freqs) / len(freqs) if freqs else 0.0

        bases = []
        maxes = []
        for i in range(self.ncpu):
            b = read_int(f"/sys/devices/system/cpu/cpu{i}/cpufreq/base_frequency", 0) or 0
            if b:
                bases.append(b)
            m = read_int(f"/sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq", 0) or 0
            if m:
                maxes.append(m)
        base_p = (max(bases) / 1000.0) if bases else 0.0
        base_e = (min(bases) / 1000.0) if bases else 0.0
        max_policy = (max(maxes) / 1000.0) if maxes else 0.0

        meminfo = {}
        for line in read_text("/proc/meminfo").splitlines():
            k, _, v = line.partition(":")
            parts = v.split()
            if parts:
                try:
                    meminfo[k] = int(parts[0]) * 1024
                except ValueError:
                    pass
        total = meminfo.get("MemTotal", 0)
        avail = meminfo.get("MemAvailable", meminfo.get("MemFree", 0))
        used = max(0, total - avail)
        memory = {
            "total": total,
            "used": used,
            "available": avail,
            "free": meminfo.get("MemFree", 0),
            "cached": meminfo.get("Cached", 0) + meminfo.get("SReclaimable", 0),
            "buffers": meminfo.get("Buffers", 0),
            "committed": meminfo.get("Committed_AS", 0),
            "commit_limit": meminfo.get("CommitLimit", 0),
            "slab": meminfo.get("Slab", 0),
            "kernel": meminfo.get("KernelStack", 0) + meminfo.get("PageTables", 0),
            "dirty": meminfo.get("Dirty", 0),
            "writeback": meminfo.get("Writeback", 0),
            "swap_total": meminfo.get("SwapTotal", 0),
            "swap_free": meminfo.get("SwapFree", 0),
            "swap_used": max(0, meminfo.get("SwapTotal", 0) - meminfo.get("SwapFree", 0)),
            "percent": (used / total * 100.0) if total else 0.0,
        }

        disk_rows = []
        disk_prev = {}
        dt = max(1e-3, now - self._disk_ts) if hasattr(self, "_disk_ts") else 1.0
        for line in read_text("/proc/diskstats").splitlines():
            f = line.split()
            if len(f) < 14:
                continue
            name = f[2]
            if name.startswith(("loop", "ram", "zram", "dm-", "sr")):
                continue
            if not os.path.isdir(f"/sys/block/{name}"):
                continue
            try:
                reads, sectors_r = int(f[3]), int(f[5])
                writes, sectors_w = int(f[7]), int(f[9])
                io_ms = int(f[12])
            except ValueError:
                continue
            cur = (reads, sectors_r, writes, sectors_w, io_ms)
            disk_prev[name] = cur
            old = self._disk_prev.get(name)
            rd = wr = active = iops = resp = 0.0
            if old:
                rd = max(0, sectors_r - old[1]) * 512 / dt
                wr = max(0, sectors_w - old[3]) * 512 / dt
                d_io = max(0, io_ms - old[4])
                d_ops = max(0, (reads - old[0]) + (writes - old[2]))
                active = min(100.0, d_io / (dt * 1000.0) * 100.0)
                iops = d_ops / dt
                resp = (d_io / d_ops) if d_ops else 0.0
            size = (read_int(f"/sys/block/{name}/size", 0) or 0) * 512
            disk_rows.append({
                "name": name, "model": self._disk_model(name), "read": rd, "write": wr,
                "active": active, "iops": iops, "response": resp, "size": size,
                "temp": self._temp("nvme") if name.startswith("nvme") and self.hwmon.get("nvme") else None,
            })
        self._disk_prev = disk_prev
        self._disk_ts = now

        net_prev = {}
        ifaces = []
        for line in read_text("/proc/net/dev").splitlines()[2:]:
            if ":" not in line:
                continue
            name, rest = line.split(":", 1)
            name = name.strip()
            f = rest.split()
            if name == "lo" or len(f) < 16:
                continue
            try:
                rx, tx = int(f[0]), int(f[8])
                rxp, txp = int(f[1]), int(f[9])
            except ValueError:
                continue
            net_prev[name] = (rx, tx)
            old = self._net_if_prev.get(name)
            rxr = max(0, rx - old[0]) / dt if old else 0.0
            txr = max(0, tx - old[1]) / dt if old else 0.0
            wireless = os.path.isdir(f"/sys/class/net/{name}/wireless")
            ifaces.append({
                "name": name, "rx": rxr, "tx": txr, "rx_total": rx, "tx_total": tx,
                "rx_packets": rxp, "tx_packets": txp,
                "mac": read_text(f"/sys/class/net/{name}/address").strip(),
                "mtu": read_int(f"/sys/class/net/{name}/mtu"),
                "state": read_text(f"/sys/class/net/{name}/operstate").strip(),
                "ipv4": iface_ipv4(name), "ipv6": iface_ipv6(name), "wireless": wireless,
                "driver": os.path.basename(os.readlink(f"/sys/class/net/{name}/device/driver"))
                if os.path.islink(f"/sys/class/net/{name}/device/driver") else None,
            })
        self._net_if_prev = net_prev

        wifi = None
        wl = next((i for i in ifaces if i["wireless"]), None)
        if wl:
            wifi = {
                "iface": wl["name"], "ssid": None, "signal_dbm": None, "signal_pct": None,
                "link_mbps": None, "freq_ghz": None, "channel": None, "ap": None,
                "txpower": None, "powersave": None, "driver": wl["driver"], "mac": wl["mac"],
                "ipv4": wl["ipv4"], "ipv6": wl["ipv6"], "temp": self._temp("wifi"),
            }
            for line in read_text("/proc/net/wireless").splitlines()[2:]:
                if ":" not in line:
                    continue
                name, rest = line.split(":", 1)
                if name.strip() != wl["name"]:
                    continue
                f = rest.replace(".", " ").split()
                try:
                    quality = float(f[1])
                    level = float(f[2])
                except (IndexError, ValueError):
                    continue
                wifi["signal_pct"] = max(0.0, min(100.0, quality / 70.0 * 100.0))
                wifi["signal_dbm"] = level
            p = run(["iwconfig", wl["name"]], 3)
            if p and p.returncode == 0:
                text = p.stdout
                m = re.search(r'ESSID:"([^"]*)"', text)
                wifi["ssid"] = m.group(1) if m else None
                m = re.search(r"Frequency:([\d.]+) GHz", text)
                if m:
                    wifi["freq_ghz"] = float(m.group(1))
                    wifi["channel"] = freq_to_channel(wifi["freq_ghz"])
                m = re.search(r"Access Point: ([0-9A-Fa-f:]{17})", text)
                wifi["ap"] = m.group(1) if m else None
                m = re.search(r"Bit Rate=([\d.]+) Mb/s", text)
                wifi["link_mbps"] = float(m.group(1)) if m else None
                m = re.search(r"Tx-Power=(\d+) dBm", text)
                wifi["txpower"] = int(m.group(1)) if m else None
                m = re.search(r"Power Management:(\w+)", text)
                wifi["powersave"] = m.group(1) if m else None

        gpu = None
        if self.gpu_card:
            card = self.gpu_card
            driver_link = os.path.join(card, "device/driver")
            driver = os.path.basename(os.readlink(driver_link)) if os.path.islink(driver_link) else "drm"
            freq = (
                read_int(os.path.join(card, "gt/gt0/rps_cur_freq_mhz"))
                or read_int(os.path.join(card, "gt_cur_freq_mhz"))
            )
            freq_act = (
                read_int(os.path.join(card, "gt_act_freq_mhz"))
                or read_int(os.path.join(card, "gt/gt0/rps_act_freq_mhz"))
            )
            freq_max = (
                read_int(os.path.join(card, "gt_max_freq_mhz"))
                or read_int(os.path.join(card, "gt/gt0/rps_max_freq_mhz"))
            )
            gpu = {
                "name": self.gpu_name, "util": self._gpu_total,
                "freq": freq,
                "freq_act": freq_act,
                "freq_max": freq_max,
                "driver": driver,
            }

        uptime = 0.0
        try:
            uptime = float(read_text("/proc/uptime").split()[0])
        except (ValueError, IndexError):
            pass
        try:
            load1, load5, load15 = os.getloadavg()
        except OSError:
            load1 = load5 = load15 = 0.0

        system = {
            "cpu": {
                "model": self.cpu_model, "util": util, "per_core": per_core,
                "freq": freq_avg, "freqs": freqs,
                "base_mhz": base_p,
                "base_e_mhz": base_e,
                "max_policy_mhz": max_policy,
                "max_mhz": self.cpu_max_mhz, "cores": self.phys_cores,
                "logical": self.ncpu, "sockets": self.sockets,
                "no_turbo": read_int("/sys/devices/system/cpu/intel_pstate/no_turbo"),
                "virtualization": bool(self.cpu_flags & {"vmx", "svm"}),
                "temp": self._temp("cpu"), "load": [load1, load5, load15],
            },
            "memory": memory,
            "disks": disk_rows,
            "volumes": self._volumes(),
            "net": {"interfaces": ifaces, "wifi": wifi},
            "gpu": gpu,
            "uptime": uptime,
            "totals": dict(self._totals),
        }

        with self.lock:
            self._system = system
            self._hist["cpu"].append(round(util, 2))
            self._hist["cores"].append([round(x, 2) for x in per_core])
            self._hist["mem"].append(round(memory["percent"], 2))
            disk = disk_rows[0] if disk_rows else None
            self._hist["disk_r"].append(round(disk["read"], 1) if disk else 0.0)
            self._hist["disk_w"].append(round(disk["write"], 1) if disk else 0.0)
            wl0 = next((i for i in ifaces if i["wireless"]), ifaces[0] if ifaces else None)
            self._hist["net_rx"].append(round(wl0["rx"], 1) if wl0 else 0.0)
            self._hist["net_tx"].append(round(wl0["tx"], 1) if wl0 else 0.0)
            self._hist["gpu"].append(round(self._gpu_total, 2))
            for k in self._hist:
                if len(self._hist[k]) > HISTORY_LEN:
                    self._hist[k] = self._hist[k][-HISTORY_LEN:]
        self._sys_ts = now

    @staticmethod
    def _volumes():
        rows = []
        seen = set()
        fstypes = {"ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "vfat", "ntfs", "ntfs3", "exfat"}
        for line in read_text("/proc/mounts").splitlines():
            f = line.split()
            if len(f) < 3:
                continue
            dev, mount, fstype = f[0], f[1], f[2]
            if fstype not in fstypes:
                continue
            mount = re.sub(r"\\([0-7]{3})", lambda m: chr(int(m.group(1), 8)), mount)
            if dev in seen:
                continue
            seen.add(dev)
            try:
                st = os.statvfs(mount)
                total = st.f_blocks * st.f_frsize
                free = st.f_bavail * st.f_frsize
            except OSError:
                continue
            rows.append({
                "device": dev, "mount": mount, "fstype": fstype, "total": total,
                "free": free, "used": max(0, total - free),
                "percent": ((total - free) / total * 100.0) if total else 0.0,
            })
        return rows

    def _proc_stat(self, pid: int):
        try:
            with open(f"/proc/{pid}/stat", "rb") as fh:
                raw = fh.read()
            lp = raw.rindex(b"(")
            rp = raw.rindex(b")")
            name = raw[lp + 1:rp].decode("utf-8", "replace")
            f = raw[rp + 2:].split()
            return {
                "name": name, "state": f[0].decode(), "ppid": int(f[1]),
                "utime": int(f[11]), "stime": int(f[12]),
                "priority": int(f[15]), "nice": int(f[16]), "threads": int(f[17]),
                "starttime": int(f[19]), "vsize": int(f[20]), "rss": int(f[21]) * PAGE_SIZE,
            }
        except (OSError, ValueError, IndexError):
            return None

    def _proc_meta(self, pid: int, starttime: int, now: float):
        m = self._meta.get(pid)
        if m and m["starttime"] == starttime and now - m["ts"] < 10:
            return m
        try:
            uid = os.stat(f"/proc/{pid}").st_uid
        except OSError:
            uid = 0
        try:
            exe = os.readlink(f"/proc/{pid}/exe")
        except OSError:
            exe = ""
        cmdline = read_text(f"/proc/{pid}/cmdline").replace("\x00", " ").strip()
        meta = {
            "starttime": starttime, "uid": uid, "user": self._uid_name(uid), "exe": exe,
            "cmdline": cmdline[:400], "app": os.path.basename(exe) if exe else None, "ts": now,
        }
        self._meta[pid] = meta
        return meta

    def _scan_gpu(self, now: float):
        pids = [int(n) for n in os.listdir("/proc") if n.isdigit()]
        engine_now = {}
        fdcount = {}
        for pid in pids:
            engines = {}
            count = 0
            try:
                with os.scandir(f"/proc/{pid}/fdinfo") as it:
                    for ent in it:
                        count += 1
                        try:
                            with open(ent.path, "rb") as fh:
                                data = fh.read()
                        except OSError:
                            continue
                        if b"drm-engine-" not in data:
                            continue
                        for line in data.splitlines():
                            if not line.startswith(b"drm-engine-"):
                                continue
                            key, _, val = line.partition(b":")
                            try:
                                ns = int(val.split()[0])
                            except (ValueError, IndexError):
                                continue
                            eng = key[11:].decode("ascii", "replace")
                            if ns > engines.get(eng, 0):
                                engines[eng] = ns
            except OSError:
                pass
            fdcount[pid] = count
            if engines:
                engine_now[pid] = engines

        interval = max(1e-3, now - self._gpu_ts) if self._gpu_ts else 0.0
        per_proc = {}
        total_pct = 0.0
        if interval > 0:
            total_ns = 0
            for pid, engines in engine_now.items():
                old = self._gpu_prev.get(pid)
                if not old:
                    continue
                best_ns = 0
                best_eng = None
                for eng, ns in engines.items():
                    if eng in old:
                        d = max(0, ns - old[eng])
                        if d > best_ns:
                            best_ns = d
                            best_eng = eng
                if best_ns > 0:
                    pct = min(100.0, best_ns / (interval * 1e9) * 100.0)
                    per_proc[pid] = (round(pct, 1), ENGINE_NAMES.get(best_eng, best_eng))
                    total_ns += best_ns
            total_pct = min(100.0, total_ns / (interval * 1e9) * 100.0)

        self._gpu_prev = engine_now
        self._gpu_ts = now
        self._gpu_proc = per_proc
        self._gpu_total = round(total_pct, 1)
        self._fdcount = fdcount

    def _scan_net(self, now: float):
        p = run(["ss", "-tunpi"], 5)
        if not p or p.returncode != 0:
            return
        interval = max(1e-3, now - self._net_ts) if self._net_ts else 0.0
        cur = {}
        up = {}
        down = {}
        meta = None
        for line in p.stdout.splitlines():
            if not line or line.startswith("Netid"):
                continue
            if line[0] in " \t":
                if not meta:
                    continue
                ms = re.search(r"bytes_sent:(\d+)", line)
                mr = re.search(r"bytes_received:(\d+)", line)
                if not ms and not mr:
                    continue
                sent = int(ms.group(1)) if ms else 0
                recv = int(mr.group(1)) if mr else 0
                key, pid = meta
                cur[key] = (pid, sent, recv)
                old = self._prev_net.get(key)
                if old and old[0] == pid:
                    du = max(0, sent - old[1])
                    dd = max(0, recv - old[2])
                    if interval > 0:
                        up[pid] = up.get(pid, 0) + du / interval
                        down[pid] = down.get(pid, 0) + dd / interval
                meta = None
                continue
            meta = None
            mu = re.search(r'users:\(\("([^"]+)",pid=(\d+),fd=(\d+)\)', line)
            if not mu:
                continue
            parts = line.split()
            if len(parts) < 6:
                continue
            pid = int(mu.group(2))
            fd = int(mu.group(3))
            meta = ((pid, fd, parts[0], parts[4], parts[5]), pid)

        self._prev_net = cur
        self._net_ts = now
        rates = {}
        with self.lock:
            for pid in set(list(up) + list(down)):
                rates[pid] = (int(up.get(pid, 0)), int(down.get(pid, 0)))
                app = self._pid_app.get(pid)
                if app:
                    tot = self._app_totals.setdefault(
                        app, {"cpu_s": 0.0, "disk_r": 0, "disk_w": 0, "up": 0, "down": 0}
                    )
                    tot["up"] += int(up.get(pid, 0) * interval)
                    tot["down"] += int(down.get(pid, 0) * interval)
            self._net_proc = rates

    def sample_processes(self):
        now = time.time()
        dt = max(1e-3, now - self._proc_ts) if self._proc_ts else 0.0
        do_gpu = (self._scan_count % 3 == 0)
        do_net = (self._scan_count % 2 == 0)
        self._scan_count += 1

        procs = []
        prev_proc = {}
        pid_app = {}
        app_delta = {}
        threads_total = 0

        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            pid = int(entry)
            st = self._proc_stat(pid)
            if not st:
                continue
            meta = self._proc_meta(pid, st["starttime"], now)
            ticks = st["utime"] + st["stime"]
            rd = wr = 0
            try:
                with open(f"/proc/{pid}/io", "rb") as fh:
                    for line in fh:
                        if line.startswith(b"read_bytes:"):
                            rd = int(line.split()[1])
                        elif line.startswith(b"write_bytes:"):
                            wr = int(line.split()[1])
            except (OSError, ValueError):
                pass
            old = self._prev_proc.get(pid)
            cpu = dr = dw = 0.0
            if old and old["starttime"] == st["starttime"]:
                if dt > 0:
                    cpu = max(0.0, min(100.0, (ticks - old["ticks"]) / CLK_TCK / dt / max(1, self.ncpu) * 100.0))
                    dr = max(0, rd - old["rd"]) / dt
                    dw = max(0, wr - old["wr"]) / dt
                app = meta["app"] or st["name"]
                acc = app_delta.setdefault(app, [0.0, 0, 0])
                acc[0] += max(0, ticks - old["ticks"]) / CLK_TCK
                acc[1] += max(0, rd - old["rd"])
                acc[2] += max(0, wr - old["wr"])
            prev_proc[pid] = {"starttime": st["starttime"], "ticks": ticks, "rd": rd, "wr": wr}
            app = meta["app"] or st["name"]
            pid_app[pid] = app
            threads_total += st["threads"]
            gpu_pct, gpu_engine = self._gpu_proc.get(pid, (0.0, None))
            net_up, net_down = self._net_proc.get(pid, (0, 0))
            procs.append({
                "pid": pid, "ppid": st["ppid"], "name": st["name"], "app": app,
                "exe": meta["exe"], "user": meta["user"], "uid": meta["uid"],
                "state": st["state"], "status": STATE_LABELS.get(st["state"], st["state"]),
                "cpu": round(cpu, 1), "mem": st["rss"], "vmem": st["vsize"],
                "threads": st["threads"], "handles": self._fdcount.get(pid, 0),
                "nice": st["nice"], "priority": st["priority"],
                "started": self.boot_time + st["starttime"] / CLK_TCK,
                "disk_r": int(dr), "disk_w": int(dw),
                "net_up": net_up, "net_down": net_down,
                "gpu": gpu_pct, "gpu_engine": gpu_engine, "cmdline": meta["cmdline"],
            })

        with self.lock:
            for app, (c, r, w) in app_delta.items():
                tot = self._app_totals.setdefault(
                    app, {"cpu_s": 0.0, "disk_r": 0, "disk_w": 0, "up": 0, "down": 0}
                )
                tot["cpu_s"] += c
                tot["disk_r"] += r
                tot["disk_w"] += w
            self._pid_app = pid_app
            self._proc_list = procs
            self._proc_ts = now
            self._totals = {
                "processes": len(procs), "threads": threads_total,
                "handles": sum(self._fdcount.values()),
            }

        if do_gpu:
            self._scan_gpu(now)
        if do_net:
            self._scan_net(now)

        self._prev_proc = prev_proc
        self._proc_samples += 1
        if len(self._meta) > len(prev_proc) * 2:
            self._meta = {p: m for p, m in self._meta.items() if p in prev_proc}

    def _apps(self):
        cur = {}
        for p in self._proc_list:
            a = cur.setdefault(p["app"], {
                "cpu": 0.0, "mem": 0, "disk_r": 0, "disk_w": 0, "up": 0, "down": 0,
                "gpu": 0.0, "procs": 0,
            })
            a["cpu"] += p["cpu"]
            a["mem"] += p["mem"]
            a["disk_r"] += p["disk_r"]
            a["disk_w"] += p["disk_w"]
            a["up"] += p["net_up"]
            a["down"] += p["net_down"]
            a["gpu"] += p["gpu"]
            a["procs"] += 1
        rows = []
        for app, tot in self._app_totals.items():
            c = cur.get(app, {})
            rows.append({
                "name": app, "cpu": min(100.0, round(c.get("cpu", 0.0), 1)), "mem": c.get("mem", 0),
                "gpu": min(100.0, round(c.get("gpu", 0.0), 1)), "procs": c.get("procs", 0),
                "disk_r": c.get("disk_r", 0), "disk_w": c.get("disk_w", 0),
                "up": c.get("up", 0), "down": c.get("down", 0),
                "cpu_s": round(tot["cpu_s"], 1), "total_disk_r": tot["disk_r"],
                "total_disk_w": tot["disk_w"], "total_up": tot["up"], "total_down": tot["down"],
            })
        rows.sort(key=lambda r: r["cpu_s"], reverse=True)
        return rows[:200]

    def _sessions_data(self):
        now = time.time()
        if now - self._sessions["ts"] < 15 and self._sessions["data"]:
            return self._sessions["data"]
        p = run(["loginctl", "list-sessions", "--no-legend"], 4)
        rows = []
        if p and p.returncode == 0:
            for line in p.stdout.splitlines():
                f = line.split()
                if len(f) < 6:
                    continue
                try:
                    uid = int(f[1])
                except ValueError:
                    continue
                rows.append({
                    "id": f[0], "uid": uid, "user": f[2],
                    "session": f[3] if f[3] != "-" else "—",
                    "tty": f[4] if f[4] != "-" else "—", "state": f[5],
                })
        self._sessions = {"ts": now, "data": rows}
        return rows

    def _users(self):
        by_uid = {}
        for p in self._proc_list:
            u = by_uid.setdefault(p["uid"], {
                "uid": p["uid"], "user": p["user"], "cpu": 0.0, "mem": 0, "disk": 0,
                "up": 0, "down": 0, "procs": 0,
            })
            u["cpu"] += p["cpu"]
            u["mem"] += p["mem"]
            u["disk"] += p["disk_r"] + p["disk_w"]
            u["up"] += p["net_up"]
            u["down"] += p["net_down"]
            u["procs"] += 1
        sessions = self._sessions_data()
        for s in sessions:
            u = by_uid.setdefault(s["uid"], {
                "uid": s["uid"], "user": s["user"], "cpu": 0.0, "mem": 0, "disk": 0,
                "up": 0, "down": 0, "procs": 0,
            })
            u["session"] = s["session"]
            u["tty"] = s["tty"]
            u["state"] = s["state"]
        allowed = {s["uid"] for s in sessions} | {MY_UID}
        rows = [u for uid, u in by_uid.items() if uid in allowed]
        for r in rows:
            r["cpu"] = min(100.0, round(r["cpu"], 1))
            r.setdefault("session", "—")
            r.setdefault("tty", "—")
            r.setdefault("state", "—")
        rows.sort(key=lambda r: r["cpu"], reverse=True)
        return rows

    def snapshot(self):
        with self.lock:
            return {
                "ts": time.time(),
                "started": self.start_ts,
                "sys_ts": self._sys_ts,
                "proc_ts": self._proc_ts,
                "proc_samples": self._proc_samples,
                "host": self._host,
                "system": self._system,
                "history": {
                    k: (list(v) if k != "cores" else [list(c) for c in v])
                    for k, v in self._hist.items()
                },
                "processes": self._proc_list,
                "apps": self._apps(),
                "users": self._users(),
                "totals": dict(self._totals),
            }

    def services(self):
        with self.lock:
            if time.time() - self._services["ts"] < 10 and self._services["data"]:
                return self._services["data"]
        rows = self._collect_services()
        with self.lock:
            self._services = {"ts": time.time(), "data": rows}
        return rows

    def _collect_services(self):
        svc_pid = {}
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            cg = read_text(f"/proc/{entry}/cgroup")
            units = re.findall(r"/([^/]+\.service)", cg)
            if not units:
                continue
            unit = units[-1]
            pid = int(entry)
            if unit not in svc_pid or pid < svc_pid[unit]:
                svc_pid[unit] = pid
        rows = []
        for scope, user in (("system", False), ("user", True)):
            base = ["systemctl"] + (["--user"] if user else [])
            units = json_cmd(base + ["list-units", "--type=service", "--all", "--output=json", "--no-pager"]) or []
            files = json_cmd(base + ["list-unit-files", "--type=service", "--output=json", "--no-pager"]) or []
            states = {u.get("unit_file"): u.get("state") for u in files if isinstance(u, dict)}
            for u in units:
                if not isinstance(u, dict):
                    continue
                unit = u.get("unit", "")
                rows.append({
                    "unit": unit, "description": u.get("description", ""),
                    "active": u.get("active", ""), "sub": u.get("sub", ""),
                    "load": u.get("load", ""), "startup": states.get(unit, ""),
                    "scope": scope, "pid": svc_pid.get(unit),
                })
        rows.sort(key=lambda r: (r["scope"], r["unit"]))
        return rows

    def startup(self):
        with self.lock:
            if time.time() - self._startup["ts"] < 30 and self._startup["data"]:
                return self._startup["data"]
        rows = self._collect_startup()
        with self.lock:
            self._startup = {"ts": time.time(), "data": rows}
        return rows

    def _collect_startup(self):
        rows = []
        base = os.path.expanduser("~/.config/autostart")
        try:
            names = sorted(os.listdir(base))
        except OSError:
            names = []
        for fn in names:
            if not fn.endswith(".desktop"):
                continue
            cfg = configparser.RawConfigParser(interpolation=None, strict=False)
            try:
                with open(os.path.join(base, fn), "r", encoding="utf-8", errors="replace") as fh:
                    cfg.read_file(fh)
            except (OSError, configparser.Error):
                continue
            if not cfg.has_section("Desktop Entry"):
                continue
            d = cfg["Desktop Entry"]
            if d.get("Type", "Application") != "Application":
                continue
            hidden = d.get("Hidden", "false").strip().lower() == "true"
            enabled = d.get("X-GNOME-Autostart-enabled", "true").strip().lower() != "false"
            rows.append({
                "name": d.get("Name", fn),
                "status": "Enabled" if (enabled and not hidden) else "Disabled",
                "type": "Autostart",
                "scope": "user",
                "command": d.get("Exec", ""),
                "description": d.get("Comment", ""),
                "source": os.path.join(base, fn),
            })
        units = json_cmd(
            ["systemctl", "--user", "list-unit-files", "--state=enabled", "--output=json", "--no-pager"], 6
        ) or []
        desc = {
            u.get("unit", ""): u.get("description", "")
            for u in (json_cmd(["systemctl", "--user", "list-units", "--all", "--output=json", "--no-pager"], 6) or [])
            if isinstance(u, dict)
        }
        for u in units:
            if not isinstance(u, dict):
                continue
            unit = u.get("unit_file", "")
            if not unit.endswith(".service"):
                continue
            rows.append({
                "name": unit,
                "status": "Enabled",
                "type": "Systemd (user)",
                "scope": "user",
                "command": "",
                "description": desc.get(unit, ""),
                "source": "",
            })
        rows.sort(key=lambda r: (r["type"] != "Autostart", r["name"].lower()))
        return rows


def fmt_bytes(n, dec=1):
    if n is None:
        return "—"
    try:
        n = float(n)
    except (TypeError, ValueError):
        return "—"
    if n != n or n in (float("inf"), float("-inf")):
        return "—"
    if abs(n) < 1024:
        return f"{round(n)} B"
    units = ["KB", "MB", "GB", "TB", "PB"]
    i = -1
    while abs(n) >= 1024 and i < len(units) - 1:
        n /= 1024
        i += 1
    d = 0 if abs(n) >= 100 else 1 if abs(n) >= 10 else dec
    return f"{n:.{d}f} {units[i]}"


def fmt_rate(n):
    return "—" if n is None else fmt_bytes(n) + "/s"


def fmt_pct(n):
    if n is None:
        return "—"
    if not n:
        return "0%"
    return f"{n:.1f}%" if n < 10 else f"{round(n)}%"


def fmt_uptime(sec):
    sec = int(sec or 0)
    d, rem = divmod(sec, 86400)
    h, rem = divmod(rem, 3600)
    m, s = divmod(rem, 60)
    return f"{d}d {h}:{m:02d}:{s:02d}" if d else f"{h}:{m:02d}:{s:02d}"


def fmt_cpu_time(sec):
    sec = int(sec or 0)
    h, rem = divmod(sec, 3600)
    m, s = divmod(rem, 60)
    return f"{h}:{m:02d}:{s:02d}"


def fmt_clock(ts):
    if not ts:
        return "—"
    return time.strftime("%H:%M", time.localtime(ts))


def nice_label(nice):
    if nice <= -10:
        return "High"
    if nice < 0:
        return "Above normal"
    if nice == 0:
        return "Normal"
    if nice <= 10:
        return "Below normal"
    return "Low"


def hex_rgb(color):
    color = color.lstrip("#")
    return tuple(int(color[i:i + 2], 16) / 255.0 for i in (0, 2, 4))


def hex_rgba(color, alpha):
    r, g, b = hex_rgb(color)
    return r, g, b, alpha


def nice_max(v):
    if not v or v <= 0:
        return 1
    import math
    exp = math.floor(math.log10(v))
    base = 10 ** exp
    f = v / base
    mult = 1 if f <= 1 else 2 if f <= 2 else 5 if f <= 5 else 10
    return mult * base


class SparkLine(Gtk.DrawingArea):
    def __init__(self, color, palette):
        super().__init__()
        self.color = color
        self.palette = palette
        self.data = []
        self.set_content_height(26)
        self.set_hexpand(True)
        self.set_draw_func(self._draw)

    def set_data(self, data):
        self.data = data or []
        self.queue_draw()

    def _draw(self, area, cr, w, h):
        if not self.data or len(self.data) < 2:
            return
        mx = max(self.data) or 1
        step = w / (len(self.data) - 1)
        cr.set_source_rgb(*hex_rgb(self.color))
        cr.set_line_width(1.4)
        first = True
        for i, v in enumerate(self.data):
            x = i * step
            y = h - 1 - min(1.0, v / mx) * (h - 2)
            if first:
                cr.move_to(x, y)
                first = False
            else:
                cr.line_to(x, y)
        cr.stroke()


class ChartArea(Gtk.DrawingArea):
    def __init__(self, palette):
        super().__init__()
        self.palette = palette
        self.series = []
        self.maxv = 100
        self.fmt = lambda v: f"{v:.0f}%"
        self.set_content_height(230)
        self.set_hexpand(True)
        self.set_draw_func(self._draw)

    def set_data(self, series, maxv, fmt):
        self.series = series
        self.maxv = maxv
        self.fmt = fmt
        self.queue_draw()

    def _draw(self, area, cr, w, h):
        if not self.series:
            return
        pal = self.palette()
        pad_l, pad_r, pad_t, pad_b = 4, 62, 10, 18
        pw = max(10, w - pad_l - pad_r)
        ph = max(10, h - pad_t - pad_b)

        maxv = self.maxv
        if maxv in (None, "auto"):
            maxv = max((max(s.get("data") or [0]) for s in self.series), default=0)
            maxv = nice_max(maxv)

        cr.select_font_face("Cantarell", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(10)
        cr.set_line_width(1)
        for i in range(5):
            y = pad_t + (ph / 4) * i
            cr.set_source_rgb(*hex_rgb(pal["chart_grid"]))
            cr.move_to(pad_l, y)
            cr.line_to(pad_l + pw, y)
            cr.stroke()
            cr.set_source_rgb(*hex_rgb(pal["chart_text"]))
            label = self.fmt(maxv * (1 - i / 4))
            cr.move_to(pad_l + pw + 7, y + 3)
            cr.show_text(label)
        cr.set_source_rgb(*hex_rgb(pal["chart_text"]))
        cr.move_to(pad_l, h - 6)
        cr.show_text("-2m")
        text_w = cr.text_extents("now").width
        cr.move_to(pad_l + pw - text_w, h - 6)
        cr.show_text("now")

        for s in self.series:
            data = s.get("data") or []
            if not data:
                continue
            color = s.get("color", pal["accent"])
            n = len(data)
            step = pw / (n - 1) if n > 1 else pw
            pts = []
            for i, v in enumerate(data):
                x = pad_l + i * step
                y = pad_t + ph - min(1.0, max(0.0, v / maxv)) * ph
                pts.append((x, y))
            if s.get("fill"):
                grad = cairo.LinearGradient(0, pad_t, 0, pad_t + ph)
                grad.add_color_stop_rgba(0, *hex_rgba(color, 0.30))
                grad.add_color_stop_rgba(1, *hex_rgba(color, 0.02))
                cr.move_to(pts[0][0], pad_t + ph)
                for x, y in pts:
                    cr.line_to(x, y)
                cr.line_to(pts[-1][0], pad_t + ph)
                cr.close_path()
                cr.set_source(grad)
                cr.fill()
            cr.set_source_rgb(*hex_rgb(color))
            cr.set_line_width(1.7)
            cr.set_line_join(cairo.LINE_JOIN_ROUND)
            cr.set_line_cap(cairo.LINE_CAP_ROUND)
            cr.move_to(*pts[0])
            for x, y in pts[1:]:
                cr.line_to(x, y)
            cr.stroke()


class CompositionBar(Gtk.DrawingArea):
    def __init__(self, palette):
        super().__init__()
        self.palette = palette
        self.values = (0, 0, 0)
        self.set_content_height(10)
        self.set_hexpand(True)
        self.set_draw_func(self._draw)

    def set_values(self, used, cached, free):
        self.values = (used, cached, free)
        self.queue_draw()

    def _draw(self, area, cr, w, h):
        used, cached, free = self.values
        total = max(1.0, used + cached + free)
        pal = self.palette()
        x = 0.0
        free_val = max(0.0, total - used - cached)
        for val, color in ((used, pal["accent"]), (cached, pal["purple"]), (free_val, pal["border"])):
            seg = max(0.0, w * val / total)
            cr.set_source_rgb(*hex_rgb(color))
            cr.rectangle(x, 0, seg, h)
            cr.fill()
            x += seg


class StatBox(Gtk.Box):
    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=1)
        self.label = Gtk.Label(xalign=0)
        self.label.add_css_class("stat-label")
        self.value = Gtk.Label(xalign=0)
        self.value.add_css_class("stat-value")
        self.sub = Gtk.Label(xalign=0)
        self.sub.add_css_class("stat-sub")
        for w in (self.label, self.value, self.sub):
            self.append(w)

    def set(self, label, value, sub=""):
        self.label.set_text(label)
        self.value.set_text(value)
        self.sub.set_text(sub)
        self.sub.set_visible(bool(sub))


class ProcHarborWindow(Gtk.ApplicationWindow):
    def __init__(self, app, sampler):
        super().__init__(application=app, title="ProcHarbor")
        self.set_icon_name(APP_ID)
        self.sampler = sampler
        self.set_default_size(1200, 720)
        self.theme = "dark"
        self.current_tab = "processes"
        self.snap = None
        self.paused = False
        self.search_text = ""
        self.selected_pid = None
        self.selected_row_type = None
        self.selected_group_name = None
        self.selected_category = None
        self.selected_group_procs = []
        self._updating = False
        self._proc_stamp = None
        self._detail_stamp = None
        self._perf_stamp = None
        self.expanded_groups = set()
        self.collapsed_groups = set()
        self.services_data = None
        self.services_ts = 0.0
        self.services_loading = False
        self.startup_data = None
        self.startup_ts = 0.0
        self.startup_loading = False
        self.sort = {
            "processes": ("cpu", -1),
            "details": ("pid", 1),
            "apps": ("cpu_s", -1),
            "users": ("cpu", -1),
            "startup": ("type", 1),
            "services": ("unit", 1),
        }
        self.columns = {k: {} for k in self.sort}
        self._rgba_cache = {}
        self._total_mem = 0
        self._apps_stamp = None
        self._users_stamp = None
        self.perf_values = {}
        self.perf_sparks = {}
        self.stat_boxes = []
        self.core_cells = []
        self._legend_item = None
        self.perf = "cpu"

        self._install_css()
        self._build_icon_map()
        self._build_ui()
        self.snap = self.sampler.snapshot()
        self._update_summary()
        self._update_status()
        self._refresh_current(force=True)
        GLib.timeout_add(1000, self._on_tick)

    def pal(self):
        return THEMES[self.theme]

    def _install_css(self):
        self.css_provider = Gtk.CssProvider()
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), self.css_provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )
        self._reload_css()

    def _reload_css(self):
        self.css_provider.load_from_data(CSS_TEMPLATE.substitute(**self.pal()).encode())
        settings = Gtk.Settings.get_default()
        if settings:
            settings.set_property("gtk-application-prefer-dark-theme", self.theme == "dark")

    def _build_icon_map(self):
        self._icon_theme = Gtk.IconTheme.get_for_display(Gdk.Display.get_default())
        self._icon_exact = {}
        self._icon_cache = {}
        self._desktop_cache = {}
        prefixes = {}
        for base in (
            os.path.expanduser("~/.local/share/applications"),
            "/usr/local/share/applications",
            "/usr/share/applications",
            "/var/lib/flatpak/exports/share/applications",
            "/var/lib/snapd/desktop/applications",
        ):
            try:
                names = sorted(os.listdir(base))
            except OSError:
                continue
            for fn in names:
                if not fn.endswith(".desktop"):
                    continue
                cfg = configparser.RawConfigParser(interpolation=None, strict=False)
                try:
                    with open(os.path.join(base, fn), "r", encoding="utf-8", errors="replace") as fh:
                        cfg.read_file(fh)
                except (OSError, configparser.Error):
                    continue
                if not cfg.has_section("Desktop Entry"):
                    continue
                d = cfg["Desktop Entry"]
                if d.get("Type", "Application") != "Application":
                    continue
                icon = (d.get("Icon") or "").strip()
                if not icon or icon.startswith("/") or not self._icon_theme.has_icon(icon):
                    continue
                tokens = {fn[:-8].lower().replace("_", "-")}
                wm = (d.get("StartupWMClass") or "").strip().lower()
                if wm:
                    tokens.add(wm.replace("_", "-"))
                exec_line = (d.get("Exec") or "").strip()
                if exec_line:
                    try:
                        parts = shlex.split(exec_line)
                    except ValueError:
                        parts = exec_line.split()
                    while parts and (os.path.basename(parts[0]) == "env" or "=" in parts[0]):
                        parts.pop(0)
                    if parts:
                        exe = os.path.basename(parts[0]).lower()
                        if exe:
                            tokens.add(exe.replace("_", "-"))
                        if exe == "flatpak":
                            for part in parts[1:]:
                                if not part.startswith("-"):
                                    tokens.add(part.lower().replace("_", "-"))
                                    break
                for tok in tokens:
                    if len(tok) < 3:
                        continue
                    self._icon_exact.setdefault(tok, icon)
                    prefixes.setdefault(tok, icon)
        self._icon_prefixes = sorted(prefixes.items(), key=lambda kv: -len(kv[0]))

    def _safe_icon(self, name, fallback="application-x-executable-symbolic"):
        if name and self._icon_theme.has_icon(name):
            return name
        if self._icon_theme.has_icon(fallback):
            return fallback
        return "application-x-executable"

    def _icon_for_app(self, app, exe, cmdline):
        app = (app or "").strip().lower()
        exe_base = os.path.basename((exe or "").rstrip("/")).lower()
        if "procharbor.py" in (cmdline or "").lower() and app.startswith("python"):
            return self._safe_icon(APP_ID, "utilities-system-monitor-symbolic")
        key = app or exe_base or "?"
        if key in self._icon_cache:
            return self._icon_cache[key]
        candidates = [c.replace("_", "-") for c in (app, exe_base) if c]
        for candidate in candidates:
            if candidate in self._icon_exact:
                icon = self._icon_exact[candidate]
                self._icon_cache[key] = icon
                return icon
        best = None
        for token, icon in self._icon_prefixes:
            for candidate in candidates:
                if len(candidate) < 4:
                    continue
                if candidate.startswith(token) or token.startswith(candidate):
                    score = min(len(token), len(candidate))
                    if best is None or score > best[0]:
                        best = (score, icon)
        if best:
            self._icon_cache[key] = best[1]
            return best[1]
        if not exe_base or app.startswith((
            "kworker", "ksoftirqd", "migration", "rcu", "irq/", "idle_inject",
            "cpuhp", "kthreadd", "kprobe", "kcompactd", "kswapd", "kdevtmpfs",
            "kauditd", "khungtaskd", "kintegrityd", "kblockd", "kthrotld",
            "scsi_eh", "nvme-", "i915", "card0-", "watchdog",
        )):
            icon = self._safe_icon("applications-system-symbolic")
        elif app.startswith("systemd") or app in (
            "dbus-daemon", "dbus-broker", "polkitd", "rtkit-daemon", "accounts-daemon",
            "avahi-daemon", "colord", "cupsd", "cups-browsed", "udisksd", "upowerd",
            "wpa_supplicant", "networkmanager", "snapd", "packagekitd",
        ):
            icon = self._safe_icon("applications-system-symbolic")
        elif app in (
            "bash", "zsh", "fish", "sh", "dash", "tmux", "screen", "python",
            "python3", "python3.10", "python3.11", "python3.12", "python3.13",
            "node", "npm", "npx", "git", "cargo", "rustc", "make", "gcc",
            "clang", "java", "perl", "ruby", "php",
        ):
            icon = self._safe_icon("utilities-terminal-symbolic")
        elif app in ("gnome-shell", "gnome-software", "gnome-control-center"):
            icon = self._safe_icon("utilities-system-monitor-symbolic")
        else:
            icon = self._safe_icon("application-x-executable-symbolic")
        self._icon_cache[key] = icon
        return icon

    def _has_desktop_match(self, app, exe, cmdline):
        if "procharbor.py" in (cmdline or "").lower() and (app or "").lower().startswith("python"):
            return True
        app = (app or "").strip().lower()
        exe_base = os.path.basename((exe or "").rstrip("/")).lower()
        key = app or exe_base or "?"
        if key in self._desktop_cache:
            return self._desktop_cache[key]
        candidates = [c.replace("_", "-") for c in (app, exe_base) if c]
        result = False
        for candidate in candidates:
            if candidate in self._icon_exact:
                result = True
                break
        if not result:
            for token, _icon in self._icon_prefixes:
                for candidate in candidates:
                    if len(candidate) >= 4 and (candidate.startswith(token) or token.startswith(candidate)):
                        result = True
                        break
                if result:
                    break
        self._desktop_cache[key] = result
        return result

    def _process_category(self, p, user_uids):
        if p.get("uid") not in user_uids:
            return "system"
        name = (p.get("app") or "").lower().replace("_", "-")
        if name.startswith("python") and "procharbor.py" in (p.get("cmdline") or "").lower():
            return "app"
        if name in (
            "sh", "bash", "dash", "zsh", "fish", "ksh", "csh", "tcsh",
            "tmux", "screen", "python", "python3", "python3.10", "python3.11",
            "python3.12", "python3.13", "node", "npm", "npx", "perl", "ruby",
            "php", "java",
        ):
            return "background"
        if name.endswith((
            "-daemon", "-service", "-server", "-agent", "-portal", "-registry",
            "-factory", "-notify", "-helper", "-launcher", "-bus", "-extension",
            "-engine", "-memconf", "-host",
        )):
            return "background"
        exe = (p.get("exe") or "").lower()
        if exe.startswith(("/usr/libexec/", "/usr/lib/systemd/", "/lib/systemd/")):
            return "background"
        if self._has_desktop_match(p["app"], p.get("exe"), p.get("cmdline")):
            return "app"
        return "background"

    def _icon_for_row(self, row):
        if not isinstance(row, dict):
            return "application-x-executable-symbolic"
        if row.get("_header"):
            return ""
        app = (row.get("name") if row.get("_group") else row.get("app")) or row.get("name") or ""
        return self._icon_for_app(app, row.get("exe") or "", row.get("cmdline") or "")

    def _icon_cell_func(self):
        def func(col, cell, model, it, _data):
            row = model.get_value(it, 0)
            if isinstance(row, dict):
                is_header = bool(row.get("_header"))
                cell.set_property("visible", not is_header)
                if not is_header:
                    cell.set_property("icon-name", self._icon_for_row(row) or "")
        return func

    def _build_ui(self):
        header = Gtk.HeaderBar()
        header.set_show_title_buttons(True)
        self.search = Gtk.SearchEntry()
        self.search.set_placeholder_text("Search processes…")
        self.search.set_size_request(340, -1)
        self.search.connect("search-changed", self._on_search)
        header.set_title_widget(self.search)
        self.pause_btn = Gtk.Button(label="Pause")
        self.pause_btn.connect("clicked", self._on_pause)
        self.end_btn = Gtk.Button(label="End task")
        self.end_btn.add_css_class("danger")
        self.end_btn.set_sensitive(False)
        self.end_btn.connect("clicked", self._on_end_task)
        header.pack_end(self.end_btn)
        header.pack_end(self.pause_btn)
        self.set_titlebar(header)

        main = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)

        side = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        side.add_css_class("sidebar")
        side.set_size_request(236, -1)
        brand = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        brand.add_css_class("brand")
        logo_path = os.path.join(HERE, "assets", "logo.svg")
        if os.path.exists(logo_path):
            icon = Gtk.Picture.new_for_filename(logo_path)
            icon.set_size_request(24, 24)
            icon.set_can_shrink(True)
            icon.set_content_fit(Gtk.ContentFit.CONTAIN)
        else:
            icon = Gtk.Image.new_from_icon_name("utilities-system-monitor-symbolic")
            icon.set_pixel_size(20)
        title = Gtk.Label(label="ProcHarbor")
        title.add_css_class("brand-title")
        brand.append(icon)
        brand.append(title)
        side.append(brand)

        self.nav = Gtk.ListBox()
        self.nav.add_css_class("nav")
        self.nav.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.nav.connect("row-selected", self._on_nav)
        self.nav_rows = {}
        for tab, icon_name, label in TABS:
            row = Gtk.ListBoxRow()
            box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=11)
            box.set_margin_top(5)
            box.set_margin_bottom(5)
            box.set_margin_start(9)
            box.set_margin_end(9)
            img = Gtk.Image.new_from_icon_name(icon_name)
            img.set_pixel_size(16)
            lbl = Gtk.Label(label=label, xalign=0)
            box.append(img)
            box.append(lbl)
            row.set_child(box)
            row.tab_id = tab
            self.nav.append(row)
            self.nav_rows[tab] = row
        side.append(self.nav)

        spacer = Gtk.Box()
        spacer.set_vexpand(True)
        side.append(spacer)

        self.theme_btn = Gtk.Button(label="Light theme")
        self.theme_btn.add_css_class("flat-theme")
        self.theme_btn.set_halign(Gtk.Align.START)
        self.theme_btn.connect("clicked", self._on_theme)
        side.append(self.theme_btn)

        self.status_label = Gtk.Label(xalign=0)
        self.status_label.add_css_class("status")
        self.status_label.set_use_markup(True)
        side.append(self.status_label)

        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.summary = Gtk.Label(xalign=0)
        self.summary.add_css_class("summary")
        self.summary.set_use_markup(True)
        content.append(self.summary)

        self.stack = Gtk.Stack()
        self.stack.set_transition_type(Gtk.StackTransitionType.NONE)
        self.stack.set_vexpand(True)
        self.stack.set_hexpand(True)
        content.append(self.stack)

        main.append(side)
        main.append(content)
        self.set_child(main)

        self._build_processes_view()
        self._build_performance_view()
        self._build_apps_view()
        self._build_startup_view()
        self._build_users_view()
        self._build_details_view()
        self._build_services_view()

        self.nav.select_row(self.nav_rows["processes"])
        self._update_status()

        key_ctrl = Gtk.EventControllerKey()
        key_ctrl.connect("key-pressed", self._on_key_pressed)
        self.add_controller(key_ctrl)

    def _on_key_pressed(self, controller, keyval, keycode, state):
        if (state & Gdk.ModifierType.CONTROL_MASK) and keyval in (Gdk.KEY_f, Gdk.KEY_F):
            if self.current_tab == "performance":
                self.nav.select_row(self.nav_rows["processes"])
            self.search.grab_focus()
            txt = self.search.get_text()
            self.search.select_region(0, len(txt))
            return True
        if keyval == Gdk.KEY_Escape:
            if self.search.has_focus():
                if self.search.get_text():
                    self.search.set_text("")
                    return True
                focus_target = getattr(self, f"{self.current_tab}_tree", None)
                if focus_target:
                    focus_target.grab_focus()
                return True
        return False

    def _scrolled(self, child):
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroll.set_vexpand(True)
        scroll.set_hexpand(True)
        scroll.set_child(child)
        return scroll

    def _new_tree(self, store, view, columns, expand_first=False, expand_last=False):
        tree = Gtk.TreeView(model=store)
        tree.set_enable_search(False)
        tree.set_headers_visible(True)
        tree.set_reorderable(False)
        tree.set_grid_lines(Gtk.TreeViewGridLines.HORIZONTAL)
        tree.set_fixed_height_mode(True)
        sel = tree.get_selection()
        sel.set_mode(Gtk.SelectionMode.SINGLE)
        if view == "processes":
            def can_select(selection, model, path, is_selected):
                it = model.get_iter(path)
                if it is not None:
                    row = model.get_value(it, 0)
                    if isinstance(row, dict) and row.get("_header"):
                        return False
                return True
            sel.set_select_function(can_select)
        sel.connect("changed", self._on_selection_changed, view)
        tree.connect("row-expanded", self._on_row_expanded)
        tree.connect("row-collapsed", self._on_row_collapsed)
        for idx, (title, key, width, align) in enumerate(columns):
            col = Gtk.TreeViewColumn(title=title)
            col.set_sizing(Gtk.TreeViewColumnSizing.FIXED)
            col.set_fixed_width(width)
            col.set_resizable(True)
            col.set_alignment(align)
            if (expand_first and idx == 0) or (expand_last and idx == len(columns) - 1):
                col.set_expand(True)
            if view == "processes" and key == "name":
                icon_renderer = Gtk.CellRendererPixbuf()
                icon_renderer.set_property("icon-size", Gtk.IconSize.NORMAL)
                icon_renderer.set_property("xalign", 0.5)
                icon_renderer.set_property("yalign", 0.5)
                icon_renderer.set_property("xpad", 0)
                icon_renderer.set_property("ypad", 0)
                icon_renderer.set_fixed_size(20, 18)
                col.pack_start(icon_renderer, False)
                col.set_cell_data_func(icon_renderer, self._icon_cell_func())
                col.set_spacing(6)
            renderer = Gtk.CellRendererText()
            renderer.set_property("ypad", 0)
            renderer.set_fixed_size(-1, 26)
            renderer.set_property("ellipsize", Pango.EllipsizeMode.END)
            col.pack_start(renderer, True)
            col.set_cell_data_func(renderer, self._cell_func(view, key, align))
            col.connect("clicked", self._on_sort_clicked, view, key)
            tree.append_column(col)
            self.columns[view][key] = col
        self._update_sort_indicators(view)
        return tree

    def _cell_func(self, view, key, align):
        def func(col, cell, model, it, _data):
            row = model.get_value(it, 0)
            if not isinstance(row, dict):
                return
            cell.set_property("text", self._cell_text(view, row, key))
            cell.set_property("xalign", align)
            is_header = bool(row.get("_header"))
            cell.set_property(
                "weight",
                Pango.Weight.BOLD if (row.get("_group") or is_header) else Pango.Weight.NORMAL,
            )
            color = self.pal()["text"]
            if view == "services" and key == "status":
                color = self._service_color_value(row)
            elif view == "startup" and key == "status":
                color = self.pal()["text"] if row.get("status") == "Enabled" else self.pal()["faint"]
            elif view in ("processes", "details") and key in ("cpu", "mem"):
                if key == "cpu":
                    color = self._load_color(row.get("cpu") or 0, True)
                else:
                    pct = ((row.get("mem") or 0) / self._total_mem * 100) if self._total_mem else 0
                    color = self._load_color(pct, False)
            elif is_header and key == "name":
                color = self.pal()["accent"]
            elif is_header:
                color = self.pal()["dim"]
            cell.set_property("foreground-rgba", self._rgba(color))
        return func

    def _load_color(self, value, is_cpu):
        pal = self.pal()
        if is_cpu:
            if value >= 50:
                return pal["danger"]
            if value >= 25:
                return pal["orange"]
            if value >= 5:
                return pal["amber"]
        else:
            if value >= 15:
                return pal["danger"]
            if value >= 8:
                return pal["orange"]
            if value >= 2:
                return pal["amber"]
        return pal["text"]

    def _mem_text(self, mem):
        mem = mem or 0
        if self._total_mem:
            return f"{fmt_bytes(mem)} · {mem / self._total_mem * 100:.0f}%"
        return fmt_bytes(mem)

    def _rgba(self, color):
        if color not in self._rgba_cache:
            rgba = Gdk.RGBA()
            rgba.parse(color)
            self._rgba_cache[color] = rgba
        return self._rgba_cache[color]

    def _service_color_value(self, row):
        if row.get("active") == "failed":
            return self.pal()["danger"]
        if row.get("active") == "active":
            return self.pal()["green"]
        return self.pal()["faint"]

    def _cell_text(self, view, row, key):
        if view == "processes":
            if key == "name":
                return row.get("name", "")
            if key == "status":
                return row.get("status", "")
            if row.get("_header"):
                return ""
            if key == "cpu":
                return fmt_pct(row.get("cpu"))
            if key == "mem":
                return self._mem_text(row.get("mem"))
            if key == "disk":
                return fmt_rate(row.get("disk") if row.get("_group") else (row.get("disk_r", 0) + row.get("disk_w", 0)))
            if key == "net":
                return fmt_rate(row.get("net") if row.get("_group") else (row.get("net_up", 0) + row.get("net_down", 0)))
            if key == "gpu":
                return fmt_pct(row.get("gpu"))
        elif view == "details":
            if key in ("pid", "name", "status", "user", "threads", "handles"):
                return str(row.get(key, ""))
            if key == "cpu":
                return fmt_pct(row.get("cpu"))
            if key == "mem":
                return self._mem_text(row.get("mem"))
            if key == "disk":
                return fmt_rate(row.get("disk_r", 0) + row.get("disk_w", 0))
            if key == "net":
                return fmt_rate(row.get("net_up", 0) + row.get("net_down", 0))
            if key == "gpu":
                return fmt_pct(row.get("gpu"))
            if key == "nice":
                return f"{row.get('nice')} · {nice_label(row.get('nice', 0))}"
            if key == "cmdline":
                return row.get("cmdline") or row.get("exe") or "—"
        elif view == "apps":
            if key == "name":
                return row.get("name", "")
            if key == "cpu_s":
                return fmt_cpu_time(row.get("cpu_s"))
            if key in ("total_down", "total_up", "total_disk_r", "total_disk_w"):
                return fmt_bytes(row.get(key))
            if key == "cpu":
                return fmt_pct(row.get("cpu"))
            if key == "mem":
                return fmt_bytes(row.get("mem"))
        elif view == "startup":
            if key == "command":
                return row.get("command") or row.get("description") or "—"
            return str(row.get(key, ""))
        elif view == "users":
            if key == "session":
                tty = row.get("tty")
                return row.get("session", "—") + (f" · {tty}" if tty and tty != "—" else "")
            if key == "cpu":
                return fmt_pct(row.get("cpu"))
            if key == "mem":
                return fmt_bytes(row.get("mem"))
            if key == "disk":
                return fmt_rate(row.get("disk"))
            if key == "net":
                return fmt_rate((row.get("up") or 0) + (row.get("down") or 0))
            if key == "procs":
                return str(row.get("procs", 0))
            return str(row.get(key, ""))
        elif view == "services":
            if key == "pid":
                return str(row.get("pid")) if row.get("pid") is not None else "—"
            if key == "status":
                return self._service_status(row)
            if key == "startup":
                return self._service_startup(row.get("startup", ""))
            return str(row.get(key, ""))
        return str(row.get(key, ""))

    def _build_processes_view(self):
        self.proc_store = Gtk.TreeStore(object)
        cols = [
            ("Name", "name", 270, 0.0),
            ("Status", "status", 130, 0.0),
            ("CPU", "cpu", 80, 1.0),
            ("Memory", "mem", 110, 1.0),
            ("Disk", "disk", 120, 1.0),
            ("Network", "net", 120, 1.0),
            ("GPU", "gpu", 70, 1.0),
        ]
        self.proc_tree = self._new_tree(self.proc_store, "processes", cols, expand_first=True)
        self.proc_scroll = self._scrolled(self.proc_tree)
        self.stack.add_named(self.proc_scroll, "processes")

    def _build_details_view(self):
        self.details_store = Gtk.ListStore(object)
        cols = [
            ("PID", "pid", 80, 1.0),
            ("Name", "name", 200, 0.0),
            ("Status", "status", 105, 0.0),
            ("User", "user", 110, 0.0),
            ("CPU", "cpu", 75, 1.0),
            ("Memory", "mem", 100, 1.0),
            ("Disk", "disk", 100, 1.0),
            ("Network", "net", 110, 1.0),
            ("GPU", "gpu", 70, 1.0),
            ("Threads", "threads", 80, 1.0),
            ("Handles", "handles", 85, 1.0),
            ("Priority", "nice", 120, 1.0),
            ("Command line", "cmdline", 420, 0.0),
        ]
        self.details_tree = self._new_tree(self.details_store, "details", cols, expand_last=True)
        self.details_scroll = self._scrolled(self.details_tree)
        self.stack.add_named(self.details_scroll, "details")

    def _build_apps_view(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        note = Gtk.Label(xalign=0)
        note.add_css_class("summary")
        self.apps_note = note
        box.append(note)
        self.apps_store = Gtk.ListStore(object)
        cols = [
            ("Name", "name", 260, 0.0),
            ("CPU time", "cpu_s", 110, 1.0),
            ("Network down", "total_down", 130, 1.0),
            ("Network up", "total_up", 130, 1.0),
            ("Disk read", "total_disk_r", 120, 1.0),
            ("Disk write", "total_disk_w", 120, 1.0),
            ("Current CPU", "cpu", 110, 1.0),
            ("Current memory", "mem", 130, 1.0),
        ]
        self.apps_tree = self._new_tree(self.apps_store, "apps", cols, expand_first=True)
        box.append(self._scrolled(self.apps_tree))
        self.stack.add_named(box, "apps")

    def _build_startup_view(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        note = Gtk.Label(label="User autostart entries and enabled systemd user services.", xalign=0)
        note.add_css_class("summary")
        self.startup_note = note
        box.append(note)
        self.startup_store = Gtk.ListStore(object)
        cols = [
            ("Name", "name", 300, 0.0),
            ("Status", "status", 100, 0.0),
            ("Type", "type", 140, 0.0),
            ("Command", "command", 420, 0.0),
        ]
        self.startup_tree = self._new_tree(self.startup_store, "startup", cols, expand_last=True)
        box.append(self._scrolled(self.startup_tree))
        self.stack.add_named(box, "startup")

    def _build_users_view(self):
        self.users_store = Gtk.ListStore(object)
        cols = [
            ("User", "user", 190, 0.0),
            ("Session", "session", 170, 0.0),
            ("Status", "state", 110, 0.0),
            ("CPU", "cpu", 90, 1.0),
            ("Memory", "mem", 130, 1.0),
            ("Disk", "disk", 130, 1.0),
            ("Network", "net", 130, 1.0),
            ("Processes", "procs", 100, 1.0),
        ]
        self.users_tree = self._new_tree(self.users_store, "users", cols, expand_first=True)
        self.stack.add_named(self._scrolled(self.users_tree), "users")

    def _build_services_view(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        note = Gtk.Label(xalign=0)
        note.add_css_class("summary")
        self.services_note = note
        box.append(note)
        self.services_store = Gtk.ListStore(object)
        cols = [
            ("Name", "unit", 260, 0.0),
            ("PID", "pid", 80, 1.0),
            ("Description", "description", 300, 0.0),
            ("Status", "status", 140, 0.0),
            ("Startup", "startup", 120, 0.0),
            ("Scope", "scope", 90, 0.0),
        ]
        self.services_tree = self._new_tree(self.services_store, "services", cols, expand_first=True)
        box.append(self._scrolled(self.services_tree))
        self.stack.add_named(box, "services")

    def _build_performance_view(self):
        outer = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        left_scroll = Gtk.ScrolledWindow()
        left_scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        left_scroll.set_size_request(232, -1)
        self.perf_list = Gtk.ListBox()
        self.perf_list.add_css_class("perf-list")
        self.perf_list.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.perf_list.connect("row-selected", self._on_perf_row)
        self.perf_rows = {}
        for item_id, name, color in PERF_ITEMS:
            row = Gtk.ListBoxRow()
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            box.set_margin_top(6)
            box.set_margin_bottom(6)
            box.set_margin_start(8)
            box.set_margin_end(8)
            top = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
            label = Gtk.Label(label=name, xalign=0)
            label.add_css_class("perf-name")
            value = Gtk.Label(label="—", xalign=1)
            value.add_css_class("muted")
            value.set_hexpand(True)
            top.append(label)
            top.append(value)
            spark = SparkLine(color, self.pal)
            box.append(top)
            box.append(spark)
            row.set_child(box)
            row.item_id = item_id
            self.perf_list.append(row)
            self.perf_rows[item_id] = row
            self.perf_values[item_id] = value
            self.perf_sparks[item_id] = spark
        left_scroll.set_child(self.perf_list)

        right_scroll = Gtk.ScrolledWindow()
        right_scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        right_scroll.set_hexpand(True)
        right_scroll.set_vexpand(True)
        detail = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        detail.set_margin_top(16)
        detail.set_margin_bottom(24)
        detail.set_margin_start(22)
        detail.set_margin_end(22)

        self.perf_title = Gtk.Label(label="CPU", xalign=0)
        self.perf_title.add_css_class("perf-title")
        self.perf_sub = Gtk.Label(label="", xalign=0)
        self.perf_sub.add_css_class("muted")
        detail.append(self.perf_title)
        detail.append(self.perf_sub)

        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        card.add_css_class("card")
        card.set_margin_top(12)
        self.perf_chart = ChartArea(self.pal)
        card.append(self.perf_chart)
        self.perf_legend = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=18)
        self.perf_legend.set_margin_top(6)
        card.append(self.perf_legend)
        detail.append(card)

        self.perf_stats_grid = Gtk.Grid(column_spacing=26, row_spacing=14)
        self.perf_stats_grid.set_margin_top(16)
        detail.append(self.perf_stats_grid)

        self.perf_extra = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        self.perf_extra.set_margin_top(16)
        detail.append(self.perf_extra)

        self.core_section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.core_section.set_margin_top(16)
        self.core_title = Gtk.Label(label="Logical processors", xalign=0)
        self.core_title.add_css_class("section-title")
        self.core_grid = Gtk.Grid(column_spacing=8, row_spacing=8)
        self.core_section.append(self.core_title)
        self.core_section.append(self.core_grid)
        detail.append(self.core_section)

        right_scroll.set_child(detail)
        outer.append(left_scroll)
        outer.append(right_scroll)
        self.perf_scroll = right_scroll
        self.stack.add_named(outer, "performance")
        self.perf_list.select_row(self.perf_rows["cpu"])

    def _on_nav(self, listbox, row):
        if row is None:
            return
        tab = row.tab_id
        if tab == self.current_tab and self.stack.get_visible_child_name() == tab:
            return
        self.current_tab = tab
        self.stack.set_visible_child_name(tab)
        self.end_btn.set_visible(tab in ("processes", "details"))
        self.end_btn.set_sensitive(False)
        self.selected_pid = None
        self.selected_row_type = None
        self.selected_group_name = None
        self.selected_category = None
        self.selected_group_procs = []
        self.search.set_visible(tab != "performance")
        placeholders = {
            "processes": "Search processes…", "apps": "Search apps…",
            "startup": "Search startup items…", "users": "Search users…",
            "details": "Search processes…", "services": "Search services…",
        }
        self.search.set_placeholder_text(placeholders.get(tab, "Search…"))
        self._refresh_current(force=True)
        if tab == "services":
            self._ensure_services()
            if self.services_data:
                self._render_services()
        if tab == "startup":
            self._ensure_startup()
            if self.startup_data:
                self._render_startup()

    def _on_search(self, entry):
        self.search_text = entry.get_text().strip().lower()
        self._refresh_current(force=True)

    def _on_sort_clicked(self, button, view, key):
        current_key, direction = self.sort[view]
        new_dir = -direction if current_key == key else (1 if key in TEXT_KEYS else -1)
        self.sort[view] = (key, new_dir)
        self._update_sort_indicators(view)
        self._refresh_current(force=True)

    def _update_sort_indicators(self, view):
        key, direction = self.sort[view]
        for col_key, col in self.columns.get(view, {}).items():
            if col_key == key:
                col.set_sort_indicator(True)
                col.set_sort_order(
                    Gtk.SortType.ASCENDING if direction > 0 else Gtk.SortType.DESCENDING
                )
            else:
                col.set_sort_indicator(False)

    def _on_row_expanded(self, tree, it, path):
        if self._updating:
            return
        row = tree.get_model().get_value(it, 0)
        if isinstance(row, dict) and row.get("_group"):
            name = row.get("name")
            self.expanded_groups.add(name)
            self.collapsed_groups.discard(name)

    def _on_row_collapsed(self, tree, it, path):
        if self._updating:
            return
        row = tree.get_model().get_value(it, 0)
        if isinstance(row, dict) and row.get("_group"):
            name = row.get("name")
            self.collapsed_groups.add(name)
            self.expanded_groups.discard(name)

    def _on_selection_changed(self, selection, view):
        if self._updating:
            return
        model, it = selection.get_selected()
        if it is None:
            self.selected_pid = None
            self.selected_row_type = None
            self.selected_group_name = None
            self.selected_category = None
            self.selected_group_procs = []
            self.end_btn.set_sensitive(False)
            return
        row = model.get_value(it, 0)
        if not isinstance(row, dict) or row.get("_header"):
            self.selected_pid = None
            self.selected_row_type = None
            self.selected_group_name = None
            self.selected_category = None
            self.selected_group_procs = []
            self.end_btn.set_sensitive(False)
            return

        if row.get("_group"):
            self.selected_row_type = "group"
            self.selected_group_name = row.get("name")
            self.selected_category = row.get("category")
            self.selected_pid = row.get("pid")
            self.selected_group_procs = list(row.get("procs") or [])
            self.end_btn.set_sensitive(self.current_tab in ("processes", "details"))
        else:
            self.selected_row_type = "process"
            self.selected_pid = row.get("pid")
            self.selected_group_name = None
            self.selected_category = None
            self.selected_group_procs = []
            self.end_btn.set_sensitive(self.selected_pid is not None and self.current_tab in ("processes", "details"))

    def _select_pid(self, tree, store, pid):
        if pid is None and not self.selected_group_name:
            return
        self._updating = True
        found = False
        try:
            selection = tree.get_selection()
            selection.unselect_all()
            it = store.get_iter_first()
            while it is not None and not found:
                row = store.get_value(it, 0)
                if isinstance(row, dict) and not row.get("_header"):
                    if self.selected_row_type == "group":
                        if row.get("_group") and row.get("name") == self.selected_group_name and row.get("category") == self.selected_category:
                            selection.select_iter(it)
                            self.selected_group_procs = list(row.get("procs") or [])
                            self.selected_pid = row.get("pid")
                            found = True
                            break
                    else:
                        if not row.get("_group") and row.get("pid") == pid:
                            selection.select_iter(it)
                            found = True
                            break
                child = store.iter_children(it)
                while child is not None and not found:
                    crow = store.get_value(child, 0)
                    if isinstance(crow, dict) and not crow.get("_group"):
                        if self.selected_row_type != "group" and crow.get("pid") == pid:
                            selection.select_iter(child)
                            found = True
                            break
                    child = store.iter_next(child)
                it = store.iter_next(it)
        finally:
            self._updating = False
        if not found:
            self.selected_pid = None
            self.selected_group_name = None
            self.selected_category = None
            self.selected_row_type = None
            self.selected_group_procs = []
            self.end_btn.set_sensitive(False)

    def _filtered_procs(self):
        procs = (self.snap or {}).get("processes") or []
        q = self.search_text
        if not q:
            return procs
        return [
            p for p in procs
            if q in p["name"].lower() or q in (p.get("app") or "").lower()
            or q in str(p.get("pid")) or q in (p.get("user") or "").lower()
            or q in (p.get("cmdline") or "").lower()
        ]

    def _build_groups(self, procs, user_uids):
        groups = {}
        for p in procs:
            category = self._process_category(p, user_uids)
            key = (p["app"], category)
            g = groups.get(key)
            if g is None:
                g = {
                    "name": p["app"], "procs": [], "cpu": 0.0, "mem": 0,
                    "disk": 0.0, "net": 0.0, "gpu": 0.0, "_group": True,
                    "category": category,
                }
                groups[key] = g
            g["procs"].append(p)
            g["cpu"] += p.get("cpu") or 0
            g["mem"] += p.get("mem") or 0
            g["disk"] += (p.get("disk_r") or 0) + (p.get("disk_w") or 0)
            g["net"] += (p.get("net_up") or 0) + (p.get("net_down") or 0)
            g["gpu"] += p.get("gpu") or 0
        for g in groups.values():
            g["cpu"] = min(100.0, round(g["cpu"], 1))
            g["gpu"] = min(100.0, g["gpu"])
            g["top"] = max(g["procs"], key=lambda p: p.get("cpu") or 0)
            g["pid"] = g["top"]["pid"]
            g["exe"] = g["top"].get("exe") or ""
            g["cmdline"] = g["top"].get("cmdline") or ""
            g["status"] = (
                f"{len(g['procs'])} processes" if len(g["procs"]) > 1 else g["top"]["status"]
            )
        return list(groups.values())

    def _group_open(self, g):
        if self.search_text:
            return True
        name = g.get("name")
        if name in self.expanded_groups:
            return True
        return False

    def _sort_groups(self, groups):
        key, direction = self.sort["processes"]
        def value(g):
            if key in TEXT_KEYS or key in ("name", "status"):
                return str(g.get(key) or "").lower()
            return g.get(key) or 0
        groups.sort(key=value, reverse=direction < 0)
        return groups

    def _refresh_processes(self, force=False):
        if not self.snap or not self.snap.get("system"):
            return
        stamp = self.snap.get("proc_ts")
        if not force and stamp == self._proc_stamp:
            return
        self._proc_stamp = stamp
        adj = self.proc_scroll.get_vadjustment()
        scroll_pos = adj.get_value()
        self._updating = True
        open_names = set()
        try:
            self.proc_store.clear()
            user_uids = {u["uid"] for u in (self.snap.get("users") or [])} or {MY_UID}
            groups = self._sort_groups(self._build_groups(self._filtered_procs(), user_uids))
            for label, cat in (("Apps", "app"), ("Background", "background"), ("System", "system")):
                section = [g for g in groups if g.get("category") == cat]
                if not section:
                    continue
                total = sum(len(g["procs"]) for g in section)
                header = {
                    "_header": True,
                    "name": label,
                    "status": f"{total} process{'es' if total != 1 else ''}",
                }
                self.proc_store.append(None, [header])
                for g in section:
                    parent = self.proc_store.append(None, [g])
                    if self._group_open(g):
                        open_names.add(g["name"])
                        for p in sorted(g["procs"], key=lambda x: -(x.get("cpu") or 0)):
                            self.proc_store.append(parent, [p])
            it = self.proc_store.get_iter_first()
            while it is not None:
                row = self.proc_store.get_value(it, 0)
                if isinstance(row, dict) and row.get("_group") and row.get("name") in open_names:
                    self.proc_tree.expand_row(self.proc_store.get_path(it), False)
                it = self.proc_store.iter_next(it)
        finally:
            self._updating = False
        self._select_pid(self.proc_tree, self.proc_store, self.selected_pid)
        GLib.idle_add(lambda: (adj.set_value(scroll_pos), False)[1])

    def _refresh_details(self, force=False):
        if not self.snap or not self.snap.get("system"):
            return
        stamp = self.snap.get("proc_ts")
        if not force and stamp == self._detail_stamp:
            return
        self._detail_stamp = stamp
        adj = self.details_scroll.get_vadjustment()
        scroll_pos = adj.get_value()
        self._updating = True
        try:
            self.details_store.clear()
            rows = self._filtered_procs()
            key, direction = self.sort["details"]
            def value(p):
                if key in ("name", "status", "user", "cmdline") or key in TEXT_KEYS:
                    return str(p.get(key) or "").lower()
                if key == "disk":
                    return (p.get("disk_r") or 0) + (p.get("disk_w") or 0)
                if key == "net":
                    return (p.get("net_up") or 0) + (p.get("net_down") or 0)
                if key == "nice":
                    return p.get("nice") or 0
                return p.get(key) or 0
            rows = sorted(rows, key=value, reverse=direction < 0)
            for p in rows:
                self.details_store.append([p])
        finally:
            self._updating = False
        self._select_pid(self.details_tree, self.details_store, self.selected_pid)
        GLib.idle_add(lambda: (adj.set_value(scroll_pos), False)[1])

    def _refresh_apps(self, force=False):
        if not self.snap:
            return
        stamp = self.snap.get("proc_ts")
        if not force and stamp == self._apps_stamp:
            return
        self._apps_stamp = stamp
        self.apps_store.clear()
        rows = (self.snap.get("apps") or [])
        q = self.search_text
        if q:
            rows = [r for r in rows if q in r["name"].lower()]
        key, direction = self.sort["apps"]
        rows = sorted(rows, key=lambda r: (r["name"].lower() if key == "name" else (r.get(key) or 0)), reverse=direction < 0)
        for r in rows:
            self.apps_store.append([r])
        self.apps_note.set_text(
            f"Since {fmt_clock(self.snap.get('started'))} · CPU time, network and disk totals are cumulative for this app session"
        )

    def _refresh_users(self, force=False):
        if not self.snap:
            return
        stamp = self.snap.get("proc_ts")
        if not force and stamp == self._users_stamp:
            return
        self._users_stamp = stamp
        self.users_store.clear()
        rows = (self.snap.get("users") or [])
        q = self.search_text
        if q:
            rows = [r for r in rows if q in r["user"].lower()]
        key, direction = self.sort["users"]
        def user_sort_val(r):
            if key in TEXT_KEYS or key in ("user", "session", "state", "tty"):
                return str(r.get(key) or "").lower()
            return r.get(key) or 0
        rows = sorted(rows, key=user_sort_val, reverse=direction < 0)
        for r in rows:
            self.users_store.append([r])

    def _refresh_current(self, force=False):
        tab = self.current_tab
        if tab == "processes":
            self._refresh_processes(force)
        elif tab == "details":
            self._refresh_details(force)
        elif tab == "apps":
            self._refresh_apps(force)
        elif tab == "users":
            self._refresh_users(force)
        elif tab == "performance":
            self._refresh_performance(force)
        elif tab == "services" and self.services_data:
            self._render_services()
        elif tab == "startup" and self.startup_data:
            self._render_startup()

    def _update_summary(self):
        if not self.snap:
            return
        totals = self.snap.get("totals") or {}
        system = self.snap.get("system") or {}
        if not system:
            return
        self._total_mem = system["memory"]["total"]
        self.summary.set_markup(
            f"Processes <b>{totals.get('processes', 0)}</b>    "
            f"Threads <b>{totals.get('threads', 0)}</b>    "
            f"Handles <b>{totals.get('handles', 0)}</b>    "
            f"CPU <b>{fmt_pct(system['cpu']['util'])}</b>    "
            f"Memory <b>{fmt_pct(system['memory']['percent'])}</b>    "
            f"Uptime <b>{fmt_uptime(system.get('uptime', 0))}</b>"
        )

    def _update_status(self):
        pal = self.pal()
        if self.paused:
            dot, text = pal["amber"], "Paused"
        elif self.snap and self.snap.get("system"):
            dot, text = pal["green"], "Live · 1s"
        else:
            dot, text = pal["faint"], "Starting…"
        self.status_label.set_markup(f'<span foreground="{dot}">●</span>  {text}')

    def _on_tick(self):
        if not self.paused:
            self.snap = self.sampler.snapshot()
            self._update_summary()
            self._update_status()
            tab = self.current_tab
            if tab == "processes":
                self._refresh_processes()
            elif tab == "details":
                self._refresh_details()
            elif tab == "apps":
                self._refresh_apps()
            elif tab == "users":
                self._refresh_users()
            elif tab == "performance":
                self._refresh_performance()
            elif tab == "services":
                self._ensure_services()
            elif tab == "startup":
                self._ensure_startup()
        return True

    def _on_pause(self, button):
        self.paused = not self.paused
        button.set_label("Resume" if self.paused else "Pause")
        self._update_status()
        if not self.paused:
            self.snap = self.sampler.snapshot()

    def _on_theme(self, button):
        self.theme = "light" if self.theme == "dark" else "dark"
        self.theme_btn.set_label("Dark theme" if self.theme == "light" else "Light theme")
        self._reload_css()
        for spark in self.perf_sparks.values():
            spark.queue_draw()
        self.perf_chart.queue_draw()
        for cell in self.core_cells:
            cell["spark"].queue_draw()
        self._refresh_current(force=True)

    def _on_perf_row(self, listbox, row):
        if row is None:
            return
        self.perf = row.item_id
        self._legend_item = None
        self._refresh_performance(force=True)

    def _refresh_performance(self, force=False):
        if not self.snap or not self.snap.get("system"):
            return
        system = self.snap["system"]
        hist = self.snap.get("history") or {}
        cpu = system["cpu"]
        mem = system["memory"]
        disk = (system.get("disks") or [{}])[0]
        gpu = system.get("gpu")
        ifaces = (system.get("net") or {}).get("interfaces") or []
        wifi_iface = next((i for i in ifaces if i.get("wireless")), None)
        wifi = (system.get("net") or {}).get("wifi")
        pal = self.pal()

        self.perf_values["cpu"].set_text(fmt_pct(cpu["util"]))
        self.perf_values["memory"].set_text(fmt_pct(mem["percent"]))
        self.perf_values["disk"].set_text(fmt_rate((disk.get("read") or 0) + (disk.get("write") or 0)))
        self.perf_values["wifi"].set_text(
            fmt_rate((wifi_iface["rx"] + wifi_iface["tx"]) if wifi_iface else 0)
        )
        self.perf_values["gpu"].set_text(fmt_pct(gpu["util"]) if gpu else "—")

        self.perf_sparks["cpu"].set_data(hist.get("cpu"))
        self.perf_sparks["memory"].set_data(hist.get("mem"))
        self.perf_sparks["disk"].set_data([
            (r or 0) + (w or 0)
            for r, w in zip(hist.get("disk_r") or [], hist.get("disk_w") or [])
        ])
        self.perf_sparks["wifi"].set_data([
            (r or 0) + (t or 0)
            for r, t in zip(hist.get("net_rx") or [], hist.get("net_tx") or [])
        ])
        self.perf_sparks["gpu"].set_data(hist.get("gpu"))

        item = self.perf
        name = next((n for i, n, c in PERF_ITEMS if i == item), item)
        self.perf_title.set_text(name)

        stats = []
        series = []
        maxv = 100
        fmt = lambda v: f"{v:.0f}%"
        legend = []
        show_cores = False

        if item == "cpu":
            self.perf_sub.set_text(f"{cpu['model']} · {cpu['cores']} cores, {cpu['logical']} logical processors")
            series = [{"data": hist.get("cpu") or [], "color": "#4cc2ff", "fill": True}]
            legend = [("Utilization", "#4cc2ff")]
            freq_list = cpu.get("freqs") or []
            max_cur = (max(freq_list) / 1000.0) if freq_list else 0.0
            stats = [
                ("Utilization", fmt_pct(cpu["util"]), f"load {cpu['load'][0]:.2f} · {cpu['load'][1]:.2f}"),
                ("Speed", f"{cpu['freq'] / 1000:.2f} GHz", f"max core {max_cur:.2f} GHz · turbo {'off' if cpu.get('no_turbo') == 1 else 'on'}"),
                ("Processes", str(self.snap["totals"]["processes"]), ""),
                ("Threads", str(self.snap["totals"]["threads"]), ""),
                ("Handles", str(self.snap["totals"]["handles"]), ""),
                ("Uptime", fmt_uptime(system.get("uptime", 0)), ""),
                ("Base speed", f"{cpu['base_mhz'] / 1000:.2f} GHz", f"P-core hardware · E-core {cpu['base_e_mhz'] / 1000:.2f} GHz"),
                ("Max speed", f"{cpu['max_policy_mhz'] / 1000:.2f} GHz", "policy cap · turbo off" if cpu.get("no_turbo") == 1 else "policy cap"),
                ("Cores", str(cpu["cores"]), ""),
                ("Logical processors", str(cpu["logical"]), ""),
                ("Sockets", str(cpu["sockets"]), ""),
                ("Virtualization", "Enabled" if cpu["virtualization"] else "Disabled", ""),
                ("Temperature", f"{cpu['temp']:.0f} °C" if cpu.get("temp") is not None else "—", "package"),
            ]
            show_cores = True
        elif item == "memory":
            self.perf_sub.set_text(f"{fmt_bytes(mem['total'])} total · {fmt_bytes(mem['available'])} available")
            used_pct = (mem["used"] / mem["total"] * 100) if mem["total"] else 0
            cached_pct = (mem["cached"] / mem["total"] * 100) if mem["total"] else 0
            series = [{"data": hist.get("mem") or [], "color": "#a68bff", "fill": True}]
            legend = [("In use", "#a68bff")]
            stats = [
                ("In use", fmt_bytes(mem["used"]), f"{used_pct:.1f}%"),
                ("Available", fmt_bytes(mem["available"]), ""),
                ("Committed", fmt_bytes(mem["committed"]), f"of {fmt_bytes(mem['commit_limit'])}"),
                ("Cached", fmt_bytes(mem["cached"]), ""),
                ("Paged pool", fmt_bytes(mem["slab"]), "Slab cache"),
                ("Non-paged pool", fmt_bytes(mem["kernel"]), "kernel stack + page tables"),
                ("Total", fmt_bytes(mem["total"]), ""),
                ("Swap used", fmt_bytes(mem["swap_used"]), f"of {fmt_bytes(mem['swap_total'])}"),
                ("Dirty", fmt_bytes(mem["dirty"]), ""),
                ("Writeback", fmt_bytes(mem["writeback"]), ""),
                ("Buffers", fmt_bytes(mem["buffers"]), ""),
                ("RAM speed", "—", "requires root (dmidecode)"),
            ]
            self._render_memory_extra(mem)
        elif item == "disk":
            self.perf_sub.set_text(f"{disk.get('model', 'No disk')} · {fmt_bytes(disk.get('size'))}")
            series = [
                {"data": hist.get("disk_r") or [], "color": "#5ad48a", "fill": True},
                {"data": hist.get("disk_w") or [], "color": "#f7b955", "fill": False},
            ]
            maxv = None
            fmt = lambda v: fmt_bytes(v, 0) + "/s"
            legend = [("Read", "#5ad48a"), ("Write", "#f7b955")]
            root_vol = next((v for v in system.get("volumes") or [] if v["mount"] == "/"), None)
            if root_vol is None and system.get("volumes"):
                root_vol = system["volumes"][0]
            stats = [
                ("Active time", fmt_pct(disk.get("active")), ""),
                ("Average response", f"{disk.get('response', 0):.1f} ms", ""),
                ("Read speed", fmt_rate(disk.get("read")), ""),
                ("Write speed", fmt_rate(disk.get("write")), ""),
                ("IOPS", f"{round(disk.get('iops', 0)):,}", ""),
                ("Capacity used", f"{fmt_bytes(root_vol['used'])} / {fmt_bytes(root_vol['total'])}" if root_vol else "—", f"{root_vol['percent']:.0f}%" if root_vol else ""),
                ("Model", disk.get("model", "—"), ""),
                ("Temperature", f"{disk['temp']:.0f} °C" if disk.get("temp") is not None else "—", "NVMe"),
            ]
            self._render_volumes_extra(system.get("volumes") or [])
        elif item == "wifi":
            self.perf_sub.set_text(f"{wifi.get('ssid') or '—'} · {wifi.get('iface')}" if wifi else "No wireless adapter")
            series = [
                {"data": hist.get("net_rx") or [], "color": "#4cc2ff", "fill": True},
                {"data": hist.get("net_tx") or [], "color": "#f7b955", "fill": False},
            ]
            maxv = None
            fmt = lambda v: fmt_bytes(v, 0) + "/s"
            legend = [("Receive", "#4cc2ff"), ("Send", "#f7b955")]
            if wifi:
                stats = [
                    ("SSID", wifi.get("ssid") or "—", ""),
                    ("Signal", f"{wifi['signal_dbm']:.0f} dBm" if wifi.get("signal_dbm") is not None else "—", f"{wifi['signal_pct']:.0f}%" if wifi.get("signal_pct") is not None else ""),
                    ("Link speed", f"{wifi['link_mbps']:.0f} Mbps" if wifi.get("link_mbps") is not None else "—", ""),
                    ("Channel", str(wifi.get("channel") or "—"), f"{wifi.get('freq_ghz')} GHz" if wifi.get("freq_ghz") else ""),
                    ("Adapter", wifi.get("iface") or "—", wifi.get("driver") or ""),
                    ("IPv4", wifi.get("ipv4") or "—", ""),
                    ("MAC", wifi.get("mac") or "—", ""),
                    ("Transmit power", f"{wifi['txpower']} dBm" if wifi.get("txpower") is not None else "—", ""),
                    ("Power save", wifi.get("powersave") or "—", ""),
                    ("Radio temp", f"{wifi['temp']:.0f} °C" if wifi.get("temp") is not None else "—", ""),
                ]
            else:
                stats = [("Adapter", "Not found", "")]
            self._render_empty_extra()
        elif item == "gpu":
            self.perf_sub.set_text(gpu["name"] if gpu else "No GPU")
            series = [{"data": hist.get("gpu") or [], "color": "#ff7676", "fill": True}]
            legend = [("Utilization", "#ff7676")]
            if gpu:
                stats = [
                    ("Utilization", fmt_pct(gpu["util"]), ""),
                    ("GPU frequency", f"{gpu['freq']} MHz" if gpu.get("freq") is not None else "—", f"max {gpu['freq_max']} MHz" if gpu.get("freq_max") else ""),
                    ("Active frequency", f"{gpu['freq_act']} MHz" if gpu.get("freq_act") is not None else "—", "actual clock"),
                    ("Driver", gpu.get("driver") or "—", ""),
                    ("Name", gpu.get("name") or "—", ""),
                    ("Memory", "Shared", "integrated GPU uses system memory"),
                ]
                self._render_gpu_extra()
            else:
                stats = [("GPU", "Not found", "")]
                self._render_empty_extra()
        else:
            self._render_empty_extra()

        if item != "memory" and item != "disk" and item != "gpu":
            self._render_empty_extra()
        if item != "cpu":
            self.core_section.set_visible(False)
        else:
            self.core_section.set_visible(True)
            self._render_core_grid(cpu, hist.get("cores") or [])

        self._set_stats(stats)
        self._set_legend(legend)
        self.perf_chart.set_data(series, maxv, fmt)

    def _set_legend(self, legend):
        if self._legend_item == tuple(legend):
            return
        self._legend_item = tuple(legend)
        child = self.perf_legend.get_first_child()
        while child:
            nxt = child.get_next_sibling()
            self.perf_legend.remove(child)
            child = nxt
        for label, color in legend:
            item = Gtk.Label()
            item.set_use_markup(True)
            item.set_markup(f'<span foreground="{color}">■</span>  {GLib.markup_escape_text(label)}')
            item.add_css_class("muted")
            self.perf_legend.append(item)

    def _set_stats(self, stats):
        if len(self.stat_boxes) != len(stats):
            for box in self.stat_boxes:
                self.perf_stats_grid.remove(box)
            self.stat_boxes = [StatBox() for _ in stats]
            for i, box in enumerate(self.stat_boxes):
                self.perf_stats_grid.attach(box, i % 4, i // 4, 1, 1)
        for box, (label, value, sub) in zip(self.stat_boxes, stats):
            box.set(label, value, sub)

    def _render_core_grid(self, cpu, hist_cores):
        cores = cpu.get("per_core") or []
        if len(self.core_cells) != len(cores):
            child = self.core_grid.get_first_child()
            while child:
                nxt = child.get_next_sibling()
                self.core_grid.remove(child)
                child = nxt
            self.core_cells = []
            for i in range(len(cores)):
                cell = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
                cell.add_css_class("core-cell")
                head = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
                label = Gtk.Label(label=f"CPU {i}", xalign=0)
                label.add_css_class("core-label")
                value = Gtk.Label(label="0%", xalign=1)
                value.add_css_class("core-label")
                value.set_hexpand(True)
                head.append(label)
                head.append(value)
                spark = SparkLine("#4cc2ff", self.pal)
                cell.append(head)
                cell.append(spark)
                self.core_grid.attach(cell, i % 6, i // 6, 1, 1)
                self.core_cells.append({"box": cell, "value": value, "spark": spark})
        freqs = cpu.get("freqs") or []
        for i, cell in enumerate(self.core_cells):
            pct = fmt_pct(cores[i])
            if i < len(freqs):
                cell["value"].set_text(f"{freqs[i] / 1000:.2f}G · {pct}")
            else:
                cell["value"].set_text(pct)
            cell["spark"].set_data(hist_cores[i] if i < len(hist_cores) else [])

    def _clear_extra(self):
        child = self.perf_extra.get_first_child()
        while child:
            nxt = child.get_next_sibling()
            self.perf_extra.remove(child)
            child = nxt

    def _render_empty_extra(self):
        self._clear_extra()

    def _render_memory_extra(self, mem):
        self._clear_extra()
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        box.add_css_class("card")
        title = Gtk.Label(label="Composition", xalign=0)
        title.add_css_class("extra-title")
        bar = CompositionBar(self.pal)
        used_pct = (mem["used"] / mem["total"] * 100) if mem["total"] else 0
        cached_pct = (mem["cached"] / mem["total"] * 100) if mem["total"] else 0
        free_pct = max(0.0, 100.0 - used_pct - cached_pct)
        bar.set_values(mem["used"], mem["cached"], mem["total"] - mem["used"] - mem["cached"])
        legend = Gtk.Label(xalign=0)
        legend.set_use_markup(True)
        legend.set_markup(
            f'<span foreground="{self.pal()["accent"]}">■</span> In use   '
            f'<span foreground="{self.pal()["purple"]}">■</span> Cached   '
            f'<span foreground="{self.pal()["border"]}">■</span> Free'
        )
        legend.add_css_class("muted")
        box.append(title)
        box.append(bar)
        box.append(legend)
        self.perf_extra.append(box)

    def _render_volumes_extra(self, volumes):
        self._clear_extra()
        if not volumes:
            return
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.add_css_class("card")
        title = Gtk.Label(label="Volumes", xalign=0)
        title.add_css_class("extra-title")
        box.append(title)
        grid = Gtk.Grid(column_spacing=18, row_spacing=6)
        headers = ["Volume", "Mount", "Type", "Used", "Free", "Total", ""]
        for c, h in enumerate(headers):
            label = Gtk.Label(label=h, xalign=1.0 if 3 <= c <= 5 else 0.0)
            label.add_css_class("faint")
            grid.attach(label, c, 0, 1, 1)
        for r, vol in enumerate(volumes, start=1):
            values = [
                (vol["device"], 0.0), (vol["mount"], 0.0), (vol["fstype"], 0.0),
                (fmt_bytes(vol["used"]), 1.0), (fmt_bytes(vol["free"]), 1.0),
                (fmt_bytes(vol["total"]), 1.0),
            ]
            for c, (text, align) in enumerate(values):
                label = Gtk.Label(label=text, xalign=align)
                label.add_css_class("vol")
                grid.attach(label, c, r, 1, 1)
            bar = Gtk.ProgressBar()
            bar.set_fraction(min(1.0, max(0.0, vol["percent"] / 100.0)))
            bar.set_size_request(110, 6)
            bar.set_valign(Gtk.Align.CENTER)
            grid.attach(bar, 6, r, 1, 1)
        box.append(grid)
        self.perf_extra.append(box)

    def _render_gpu_extra(self):
        self._clear_extra()
        procs = sorted(
            [p for p in (self.snap.get("processes") or []) if (p.get("gpu") or 0) > 0],
            key=lambda p: -p["gpu"],
        )[:6]
        if not procs:
            return
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.add_css_class("card")
        title = Gtk.Label(label="Processes using the GPU", xalign=0)
        title.add_css_class("extra-title")
        box.append(title)
        grid = Gtk.Grid(column_spacing=14, row_spacing=7)
        for r, p in enumerate(procs):
            name = Gtk.Label(label=f"{p['name']}  ({p['pid']}{' · ' + p['gpu_engine'] if p.get('gpu_engine') else ''})", xalign=0)
            name.add_css_class("vol")
            bar = Gtk.ProgressBar()
            bar.set_fraction(min(1.0, max(0.0, p["gpu"] / 100.0)))
            bar.set_size_request(160, 6)
            bar.set_valign(Gtk.Align.CENTER)
            pct = Gtk.Label(label=fmt_pct(p["gpu"]), xalign=1)
            pct.add_css_class("vol-dim")
            grid.attach(name, 0, r, 1, 1)
            grid.attach(bar, 1, r, 1, 1)
            grid.attach(pct, 2, r, 1, 1)
        box.append(grid)
        self.perf_extra.append(box)

    def _service_status(self, r):
        if r.get("active") == "failed":
            return "Failed"
        if r.get("active") == "active":
            return "Running" if r.get("sub") == "running" else f"Active ({r.get('sub')})"
        if r.get("active") == "activating":
            return "Starting"
        if r.get("active") == "deactivating":
            return "Stopping"
        return "Stopped"

    def _service_startup(self, state):
        return {
            "enabled": "Automatic", "disabled": "Disabled", "static": "Manual",
            "indirect": "Manual", "generated": "Manual",
        }.get(state, "—")

    def _ensure_services(self, force=False):
        if self.services_loading:
            return
        if not force and self.services_data is not None and time.time() - self.services_ts < 10:
            return
        self.services_loading = True
        def work():
            data = self.sampler.services()
            GLib.idle_add(self._services_loaded, data)
        threading.Thread(target=work, daemon=True).start()

    def _services_loaded(self, data):
        self.services_data = data
        self.services_ts = time.time()
        self.services_loading = False
        if self.current_tab == "services":
            self._render_services()
        return False

    def _render_services(self):
        if not self.services_data:
            self.services_note.set_text("Loading services…")
            return
        rows = self.services_data
        q = self.search_text
        if q:
            rows = [
                r for r in rows
                if q in r["unit"].lower() or q in (r.get("description") or "").lower()
            ]
        key, direction = self.sort["services"]
        def sort_val(r):
            if key == "unit":
                return r.get("unit", "").lower()
            if key == "pid":
                return r.get("pid") if r.get("pid") is not None else -1
            if key == "status":
                return self._service_status(r)
            if key == "startup":
                return self._service_startup(r.get("startup", ""))
            return str(r.get(key) or "").lower()
        rows = sorted(rows, key=sort_val, reverse=direction < 0)
        self.services_store.clear()
        for r in rows:
            self.services_store.append([r])
        running = sum(1 for r in self.services_data if r.get("active") == "active" and r.get("sub") == "running")
        self.services_note.set_text(f"{running} running · {len(self.services_data)} total · systemd")

    def _ensure_startup(self, force=False):
        if self.startup_loading:
            return
        if not force and self.startup_data is not None and time.time() - self.startup_ts < 30:
            return
        self.startup_loading = True
        def work():
            data = self.sampler.startup()
            GLib.idle_add(self._startup_loaded, data)
        threading.Thread(target=work, daemon=True).start()

    def _startup_loaded(self, data):
        self.startup_data = data
        self.startup_ts = time.time()
        self.startup_loading = False
        if self.current_tab == "startup":
            self._render_startup()
        return False

    def _render_startup(self):
        rows = self.startup_data or []
        q = self.search_text
        if q:
            rows = [
                r for r in rows
                if q in r["name"].lower() or q in (r.get("command") or "").lower()
                or q in (r.get("description") or "").lower()
            ]
        key, direction = self.sort["startup"]
        def startup_sort_val(r):
            if key == "command":
                return (r.get("command") or r.get("description") or "").lower()
            return str(r.get(key) or "").lower()
        rows = sorted(rows, key=startup_sort_val, reverse=direction < 0)
        self.startup_store.clear()
        for r in rows:
            self.startup_store.append([r])

    def _find_proc(self, pid):
        for p in (self.snap or {}).get("processes") or []:
            if p["pid"] == pid:
                return p
        return None

    def _on_end_task(self, button):
        if self.selected_row_type == "group" and self.selected_group_name:
            procs = self.selected_group_procs or []
            name = self.selected_group_name
            dialog = Gtk.AlertDialog()
            if len(procs) > 1:
                dialog.set_message(f'End "{name}" ({len(procs)} processes)?')
                dialog.set_detail(f"All {len(procs)} processes will receive SIGTERM.")
            else:
                dialog.set_message(f'End "{name}"?')
                dialog.set_detail(f"PID {self.selected_pid} will receive SIGTERM.")
            dialog.set_buttons(["Cancel", "End task"])
            dialog.set_cancel_button(0)
            dialog.set_default_button(0)
            dialog.choose(self, None, self._on_end_choice, (self.selected_pid, procs))
            return

        pid = self.selected_pid
        if not pid:
            return
        proc = self._find_proc(pid)
        name = proc["name"] if proc else f"PID {pid}"
        dialog = Gtk.AlertDialog()
        dialog.set_message(f'End "{name}"?')
        dialog.set_detail(f"PID {pid} will receive SIGTERM.")
        dialog.set_buttons(["Cancel", "End task"])
        dialog.set_cancel_button(0)
        dialog.set_default_button(0)
        dialog.choose(self, None, self._on_end_choice, (pid, [proc] if proc else []))

    def _on_end_choice(self, dialog, result, target):
        try:
            choice = dialog.choose_finish(result)
        except GLib.Error:
            return
        if choice != 1:
            return
        pid, procs = target
        pids_to_kill = [p["pid"] for p in procs if p and p.get("pid")] if procs else ([pid] if pid else [])
        errors = []
        for p in pids_to_kill:
            if p <= 1:
                continue
            try:
                st = os.stat(f"/proc/{p}")
            except OSError:
                continue
            if st.st_uid != MY_UID and MY_UID != 0:
                errors.append(f"PID {p}: Not owned by your user")
                continue
            try:
                os.kill(p, signal.SIGTERM)
            except OSError as exc:
                errors.append(f"PID {p}: {exc.strerror}")
        if errors and len(errors) == len(pids_to_kill):
            self._error_dialog("\n".join(errors[:5]))
        self.selected_pid = None
        self.selected_group_name = None
        self.selected_category = None
        self.selected_row_type = None
        self.selected_group_procs = []
        self.end_btn.set_sensitive(False)
        GLib.timeout_add(400, lambda: (self._refresh_processes(force=True), False)[1])

    def _error_dialog(self, message):
        dialog = Gtk.AlertDialog()
        dialog.set_message("ProcHarbor")
        dialog.set_detail(message)
        dialog.set_buttons(["OK"])
        dialog.show(self)


class ProcHarborApp(Gtk.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID)
        self.sampler = Sampler()

    def do_startup(self):
        Gtk.Application.do_startup(self)
        GLib.set_application_name("ProcHarbor")
        GLib.set_prgname(APP_ID)
        Gtk.Window.set_default_icon_name(APP_ID)
        self.sampler.start()

    def do_activate(self):
        window = self.get_active_window()
        if window:
            window.present()
            return
        deadline = time.time() + 3.0
        while time.time() < deadline:
            snap = self.sampler.snapshot()
            if snap.get("system") and snap.get("processes") and snap.get("proc_samples", 0) >= 2:
                break
            time.sleep(0.1)
        window = ProcHarborWindow(self, self.sampler)
        window.present()


def main():
    if len(sys.argv) > 1 and sys.argv[1] in ("-v", "--version"):
        print(f"ProcHarbor {__version__}")
        return 0
    if not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY"):
        print("No graphical session detected (DISPLAY/WAYLAND_DISPLAY unset).", file=sys.stderr)
        return 1
    app = ProcHarborApp()
    return app.run(sys.argv)


if __name__ == "__main__":
    sys.exit(main())

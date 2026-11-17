<p align="center">
  <img src="assets/logo.svg" width="104" alt="ProcHarbor logo">
</p>

<h1 align="center">ProcHarbor</h1>

<p align="center">A native GTK4 task manager and system monitor for Linux.</p>

ProcHarbor is an independent Linux desktop task manager built with GTK4 and
Python. It covers the tabs people expect from a task manager — processes,
performance, startup apps, users, details and services — using `/proc`, `/sys`,
systemd and DRM fdinfo directly. No root, no daemon, no network access.

![ProcHarbor screenshot](assets/screenshot.png)

## Features

- **Processes** — grouped by executable, collapsible groups, sortable columns,
  search, per-process CPU, memory, disk, network and GPU, End task.
- **Performance** — CPU (per-core clocks and charts, package temperature),
  Memory, Disk (volumes, NVMe temperature), Wi-Fi (signal, link rate, adapter,
  power save) and GPU, all with live charts.
- **App history** — cumulative CPU time, network and disk totals per app.
- **Startup** — user-added entries from `~/.config/autostart`, with enabled or
  disabled status.
- **Users** — login sessions with per-user CPU, memory, disk, network and
  process counts.
- **Details** — flat process table: PID, status, user, CPU, memory, disk,
  network, GPU, threads, handles, priority and command line.
- **Services** — systemd units with main PID, state, startup type and scope.
- Search, column sorting, pause, end task, light/dark theme and `Ctrl+F`.

## Install

### Debian package

Download the `.deb` for your architecture from
[Releases](https://github.com/shripad-jyothinath/procharbor/releases):

```sh
# x86_64
sudo apt install ./procharbor_1.0.0_amd64.deb

# arm64
sudo apt install ./procharbor_1.0.0_arm64.deb
```

Launch **ProcHarbor** from the application menu, or run:

```sh
procharbor
```

### From source

Requires Python 3.8+, PyGObject, GTK4 and pycairo:

```sh
./run.sh
```

Install a menu entry and desktop shortcut for the source checkout:

```sh
./install-shortcut.sh
```

## Build the Debian packages

```sh
./build-deb.sh
```

This creates `dist/procharbor_1.0.0_amd64.deb`,
`dist/procharbor_1.0.0_arm64.deb` and `dist/SHA256SUMS`. The packages contain
the same pure-Python application and icons; the architecture-specific metadata
is provided for each target.

Build requirements: `dpkg-deb`, Python 3 with GdkPixbuf (`python3-gi`).

## Linux mappings

- **Handles** are open file descriptors.
- **Paged pool** maps to Slab; **non-paged pool** to kernel stack + page tables.
- **Disk per process** comes from `/proc/<pid>/io`; other users' processes are
  unreadable and show 0.
- **Network per process** is attributed from `ss -tunpi`, so it covers processes
  you own and is sampled every 4 seconds.
- **GPU utilization** comes from DRM `drm-engine-*` counters in
  `/proc/<pid>/fdinfo`, sampled every 6 seconds. Integrated GPUs report
  "Shared" memory.
- **Startup impact** cannot be measured on Linux; the tab shows user-added
  autostart entries with their enabled/disabled status.
- **RAM speed / DIMM slots** need `dmidecode` as root and are shown as "—".

## Performance

The process tables use GTK fixed-height rows, so only visible rows are
rendered. Idle cost is roughly the sampler alone — about 4–6% of one core —
including per-process GPU and network collection.

## License

MIT. ProcHarbor is not affiliated with Microsoft or the Windows Task Manager.

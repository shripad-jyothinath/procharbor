"""Unit and integration tests for ProcHarbor."""

import os
import sys
import unittest
import tempfile
import shutil

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import procharbor


class TestFormatters(unittest.TestCase):
    def test_fmt_bytes(self):
        self.assertEqual(procharbor.fmt_bytes(None), "—")
        self.assertEqual(procharbor.fmt_bytes("invalid"), "—")
        self.assertEqual(procharbor.fmt_bytes(float("nan")), "—")
        self.assertEqual(procharbor.fmt_bytes(float("inf")), "—")
        self.assertEqual(procharbor.fmt_bytes(0), "0 B")
        self.assertEqual(procharbor.fmt_bytes(512), "512 B")
        self.assertEqual(procharbor.fmt_bytes(1024), "1.0 KB")
        self.assertEqual(procharbor.fmt_bytes(10 * 1024 * 1024), "10.0 MB")
        self.assertEqual(procharbor.fmt_bytes(2.5 * 1024 * 1024 * 1024), "2.5 GB")
        self.assertEqual(procharbor.fmt_bytes(1024 * 1024 * 1024 * 1024), "1.0 TB")

    def test_fmt_rate(self):
        self.assertEqual(procharbor.fmt_rate(None), "—")
        self.assertEqual(procharbor.fmt_rate(1024), "1.0 KB/s")

    def test_fmt_pct(self):
        self.assertEqual(procharbor.fmt_pct(None), "—")
        self.assertEqual(procharbor.fmt_pct(0), "0%")
        self.assertEqual(procharbor.fmt_pct(4.24), "4.2%")
        self.assertEqual(procharbor.fmt_pct(75.8), "76%")

    def test_fmt_uptime(self):
        self.assertEqual(procharbor.fmt_uptime(0), "0:00:00")
        self.assertEqual(procharbor.fmt_uptime(65), "0:01:05")
        self.assertEqual(procharbor.fmt_uptime(3665), "1:01:05")
        self.assertEqual(procharbor.fmt_uptime(90065), "1d 1:01:05")

    def test_fmt_cpu_time(self):
        self.assertEqual(procharbor.fmt_cpu_time(0), "0:00:00")
        self.assertEqual(procharbor.fmt_cpu_time(72), "0:01:12")
        self.assertEqual(procharbor.fmt_cpu_time(3600), "1:00:00")

    def test_fmt_clock(self):
        self.assertEqual(procharbor.fmt_clock(None), "—")
        self.assertRegex(procharbor.fmt_clock(1700000000), r"^\d{2}:\d{2}$")

    def test_nice_label(self):
        self.assertEqual(procharbor.nice_label(-20), "High")
        self.assertEqual(procharbor.nice_label(-10), "High")
        self.assertEqual(procharbor.nice_label(-5), "Above normal")
        self.assertEqual(procharbor.nice_label(0), "Normal")
        self.assertEqual(procharbor.nice_label(5), "Below normal")
        self.assertEqual(procharbor.nice_label(15), "Low")

    def test_hex_rgb_and_rgba(self):
        rgb = procharbor.hex_rgb("#ffffff")
        self.assertEqual(rgb, (1.0, 1.0, 1.0))
        rgba = procharbor.hex_rgba("#000000", 0.5)
        self.assertEqual(rgba, (0.0, 0.0, 0.0, 0.5))

    def test_nice_max(self):
        self.assertEqual(procharbor.nice_max(0), 1)
        self.assertEqual(procharbor.nice_max(-10), 1)
        self.assertEqual(procharbor.nice_max(8), 10)
        self.assertEqual(procharbor.nice_max(15), 20)
        self.assertEqual(procharbor.nice_max(42), 50)
        self.assertEqual(procharbor.nice_max(88), 100)


class TestSampler(unittest.TestCase):
    def setUp(self):
        self.sampler = procharbor.Sampler()

    def test_sample_system(self):
        self.sampler.sample_system()
        snap = self.sampler.snapshot()
        self.assertIn("system", snap)
        sys = snap["system"]
        self.assertIn("cpu", sys)
        self.assertIn("memory", sys)
        self.assertIn("disks", sys)
        self.assertIn("volumes", sys)
        self.assertIn("net", sys)
        self.assertGreater(sys["memory"]["total"], 0)

    def test_sample_processes(self):
        self.sampler.sample_processes()
        snap = self.sampler.snapshot()
        self.assertIn("processes", snap)
        self.assertIsInstance(snap["processes"], list)
        self.assertGreater(len(snap["processes"]), 0)
        proc = snap["processes"][0]
        self.assertIn("pid", proc)
        self.assertIn("name", proc)
        self.assertIn("cpu", proc)
        self.assertIn("mem", proc)

    def test_services_collection(self):
        services = self.sampler.services()
        self.assertIsInstance(services, list)
        if services:
            self.assertIn("unit", services[0])
            self.assertIn("scope", services[0])

    def test_startup_collection_graceful_missing_autostart(self):
        # Verify that even when ~/.config/autostart is missing or empty, startup() collects services
        orig_expanduser = os.path.expanduser
        try:
            # Point ~/.config/autostart to non-existent directory
            os.path.expanduser = lambda path: "/non/existent/path/autostart" if "autostart" in path else orig_expanduser(path)
            startup_items = self.sampler._collect_startup()
            self.assertIsInstance(startup_items, list)
            # If system has systemd user services, they should still be returned
        finally:
            os.path.expanduser = orig_expanduser

    def test_volumes_octal_escaping(self):
        # Verify that _volumes handles mount paths with octal spaces
        vols = self.sampler._volumes()
        self.assertIsInstance(vols, list)
        for v in vols:
            self.assertNotIn(r"\040", v["mount"])


class TestVersionAndCli(unittest.TestCase):
    def test_version_defined(self):
        self.assertTrue(hasattr(procharbor, "__version__"))
        self.assertEqual(procharbor.__version__, "1.3.0")


if __name__ == "__main__":
    unittest.main()

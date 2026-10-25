"""Opt-in real Chrome/Browser Harness smoke; never invokes a model API.

Run using the separately installed Jev venv, from the repository root:
  PANDACODE_JEV_BROWSER_SMOKE=1 <venv>/bin/python tests/jev_browser_smoke.py
"""
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

spec = importlib.util.spec_from_file_location("pandacode_jev", Path(__file__).resolve().parents[1] / "src-tauri/scripts/jev_runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


@unittest.skipUnless(os.environ.get("PANDACODE_JEV_BROWSER_SMOKE") == "1", "opt-in installed Chrome smoke")
class BrowserSmoke(unittest.TestCase):
    def test_private_mobile_browser_click_assertions_and_cleanup_without_model_calls(self):
        class Page(BaseHTTPRequestHandler):
            def do_GET(self):
                content = b'''<!doctype html><meta name="viewport" content="width=device-width"><title>PandaCode Jev fixture</title>
                <button onclick="this.textContent='Verified click'">Increment fixture</button><p>Private browser fixture</p>'''
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(content)))
                self.end_headers()
                self.wfile.write(content)

            def log_message(self, *_):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Page)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        old_environment = dict(os.environ)
        runtime = None
        preview = None
        try:
            with tempfile.TemporaryDirectory(prefix="pandacode-jev-smoke-") as output, tempfile.TemporaryDirectory(prefix="bj-native-", dir="/tmp" if os.name != "nt" else None) as browser_dir:
                url = "http://127.0.0.1:%s/" % server.server_port
                request = runner.validate_request(dict(runId=str(uuid.uuid4()), url=url,
                    goal="Inspect the local fixture", maxSteps=2, timeoutSeconds=60, viewport="mobile",
                    expectedText="Verified click", expectedTitle="PandaCode Jev fixture", expectedUrl=url,
                    typesafeKey="unused-fixture-key", textKey="", textModel="unused-fixture-model",
                    textBaseUrl="https://example.invalid/v1", chromePath="", outputDir=output, browserDir=browser_dir))
                diagnostic = runner.doctor()
                self.assertTrue(diagnostic["ready"], diagnostic["issues"])
                runtime = runner.Runtime(request, diagnostic["chromePath"])
                agent = runtime.start()
                # No run(), predict(), tick(), or model client call is used.
                browser = agent.browser
                dimensions = browser.evaluate("({width:innerWidth,height:innerHeight})")
                self.assertEqual(dimensions["width"], 390)
                stream = io.StringIO()
                output_stream = runner.Output(stream)
                preview = runner.LivePreview(browser, request, output_stream, time.monotonic())
                preview.start()
                marker = Path(browser_dir) / "live-watch.json"
                def lease(duration=10000):
                    marker.write_text(json.dumps({"runId": request["runId"], "expiresAt": int(time.time() * 1000) + duration}))
                def frames():
                    with output_stream.lock:
                        return [json.loads(line) for line in stream.getvalue().splitlines()]
                def await_frames(count):
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline:
                        if len(frames()) >= count:
                            return frames()
                        time.sleep(0.05)
                    self.fail("Expected live isolated-browser frame did not arrive")
                self.assertFalse((Path(browser_dir) / "live-frame.jpg").exists())
                lease()
                first = await_frames(1)[0]
                self.assertEqual((first["width"], first["height"]), (390, 844))
                self.assertEqual(first["sha256"], runner.hashlib.sha256((Path(browser_dir) / "live-frame.jpg").read_bytes()).hexdigest())
                page = browser.observe(screenshot=False)
                action = next(item for item in page["actions"] if item["kind"] == "click" and "Increment fixture" in str(item))
                browser.act(action, page)
                next_frames = await_frames(2)
                self.assertNotEqual(next_frames[-1]["sha256"], first["sha256"])
                # Expiry must stop sampling even when a view disappears without
                # an explicit unwatch call; the next lease resumes immediately.
                lease(-1)
                time.sleep(0.7)
                count = len(frames())
                browser.evaluate("document.body.style.backgroundColor='rgb(0, 180, 100)'")
                time.sleep(0.7)
                self.assertEqual(len(frames()), count)
                lease()
                await_frames(count + 1)
                preview.close()
                self.assertFalse(preview.thread.is_alive())
                self.assertFalse(marker.exists())
                self.assertFalse((Path(browser_dir) / "live-frame.jpg").exists())
                preview = None
                verification, checks, _ = runner.verification(browser, request)
                self.assertEqual(verification, "passed", checks)
                self.assertEqual(len(checks), 3)
                self.assertEqual(runner.capture_screenshot(browser, request), "final.jpg")
                self.assertGreater((Path(output) / "final.jpg").stat().st_size, 100)
                chrome, daemon = runtime.chrome, runtime.daemon
                profile_root = Path(runtime.temporary.name)
                runtime.close()
                runtime = None
                self.assertIsNotNone(chrome.poll())
                self.assertIsNotNone(daemon.poll())
                self.assertFalse(profile_root.exists())
        finally:
            if preview:
                preview.close()
            if runtime:
                runtime.close()
            server.shutdown()
            server.server_close()
            os.environ.clear()
            os.environ.update(old_environment)


if __name__ == "__main__":
    unittest.main(verbosity=2)

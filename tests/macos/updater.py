#!/usr/bin/env python3
"""Run after ./macos/bundle.sh with: uv run tests/macos/updater.py.

Uses an isolated app, home, signing key and loopback feed. No Keychain access
or published release is involved. The replacement executable records relaunch.
"""
import functools
import http.server
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout.strip()


def wait_for(predicate, process, log):
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if predicate():
            return
        if process.poll() is not None:
            # Relaunch can finish just after the original process terminates.
            time.sleep(1)
            if predicate():
                return
            raise AssertionError(f"App exited early: {log.read_text()}")
        time.sleep(0.1)
    raise AssertionError(f"Timed out: {log.read_text()}")


def main():
    with tempfile.TemporaryDirectory(prefix="minterm-updater-") as directory:
        work = Path(directory)
        bundle_id = f"com.github.alexfdez1010.minterm.qa.{uuid.uuid4().hex}"
        key_script = work / "key.swift"
        key_script.write_text('''import CryptoKit
import Foundation
let key = Curve25519.Signing.PrivateKey()
try key.rawRepresentation.base64EncodedString().write(toFile: CommandLine.arguments[1], atomically: true, encoding: .utf8)
try key.publicKey.rawRepresentation.base64EncodedString().write(toFile: CommandLine.arguments[2], atomically: true, encoding: .utf8)
''')
        run("swift", str(key_script), str(work / "key"), str(work / "public"))
        sign_script = work / "sign.swift"
        sign_script.write_text('''import CryptoKit
import Foundation
let seed = Data(base64Encoded: try String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8))!
let key = try Curve25519.Signing.PrivateKey(rawRepresentation: seed)
print(try key.signature(for: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[2]))).base64EncodedString())
''')
        old = work / "installed/Minterm.app"
        new = work / "release/Minterm.app"
        old.parent.mkdir()
        new.parent.mkdir()
        run("ditto", str(ROOT / "dist/Minterm.app"), str(old))
        run("ditto", str(old), str(new))
        home = work / "home"
        preferences = home / "Library/Preferences"
        preferences.mkdir(parents=True)
        (preferences / "com.github.alexfdez1010.minterm.json").write_text(
            json.dumps({"automaticUpdates": True})
        )
        shell_started = work / "shell-started"
        shell = work / "shell"
        shell.write_text(f"#!/bin/sh\nprintf started > '{shell_started}'\n")
        shell.chmod(0o755)
        relaunched = work / "relaunched"
        replacement = work / "replacement.m"
        replacement.write_text('''#import <Foundation/Foundation.h>
int main() { @autoreleasepool {
    NSData *result = [NSJSONSerialization dataWithJSONObject:@{
        @"version": NSBundle.mainBundle.infoDictionary[@"CFBundleVersion"],
        @"arguments": NSProcessInfo.processInfo.arguments
    } options:0 error:nil];
    [result writeToFile:@"''' + str(relaunched) + '''" atomically:YES];
} return 0; }
''')
        run("clang", "-fobjc-arc", "-framework", "Foundation", str(replacement), "-o", str(new / "Contents/MacOS/minterm"))

        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def copyfile(self, source, output):
                if self.path.endswith(".dmg"):
                    # Leave time to inspect the native download progress window.
                    while chunk := source.read(65536):
                        output.write(chunk)
                        time.sleep(0.15)
                else:
                    super().copyfile(source, output)

        server = http.server.ThreadingHTTPServer(
            ("127.0.0.1", 0), functools.partial(Handler, directory=str(work))
        )
        feed_url = f"http://127.0.0.1:{server.server_port}"
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (new / "Contents/Resources/fixture.bin").write_bytes(os.urandom(2 * 1024 * 1024))
        for app, version in [(old, "1"), (new, "2")]:
            info_path = app / "Contents/Info.plist"
            info = plistlib.loads(info_path.read_bytes())
            info.update(CFBundleIdentifier=bundle_id, CFBundleVersion=version,
                        CFBundleShortVersionString=f"0.0.{version}",
                        SUPublicEDKey=(work / "public").read_text(),
                        SUFeedURL=f"{feed_url}/appcast.xml")
            info_path.write_bytes(plistlib.dumps(info))
            run("codesign", "--force", "--sign", "-", str(app))
        archive = work / "update.dmg"
        run("hdiutil", "create", "-quiet", "-volname", "Minterm", "-srcfolder", str(new.parent), "-fs", "HFS+", "-format", "UDZO", str(archive))
        signature = run("swift", str(sign_script), str(work / "key"), str(archive))
        feed = work / "appcast.xml"

        def write_feed(version):
            feed.write_text(f'''<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
<channel><title>Minterm test</title><item><sparkle:version>{version}</sparkle:version>
<sparkle:shortVersionString>0.0.{version}</sparkle:shortVersionString>
<enclosure url="{feed_url}/update.dmg" length="{archive.stat().st_size}" type="application/octet-stream" sparkle:edSignature="{signature}" />
</item></channel></rss>''')

        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), SHELL=str(shell))
        process = None
        try:
            # No update must open the workspace without a redundant alert.
            write_feed("1")
            log = work / "app.log"
            with log.open("w") as output:
                process = subprocess.Popen([str(old / "Contents/MacOS/minterm"), str(work)], env=env, stdout=output, stderr=output)
            wait_for(shell_started.exists, process, log)
            process.terminate()
            process.wait(timeout=10)
            shell_started.unlink()
            print("PASS: no update opens the terminal", flush=True)

            write_feed("2")
            with log.open("w") as output:
                process = subprocess.Popen([str(old / "Contents/MacOS/minterm"), str(work)], env=env, stdout=output, stderr=output)
            print(f"Installing signed fixture from {old}", flush=True)
            wait_for(relaunched.exists, process, log)
            result = json.loads(relaunched.read_text())
            assert result["version"] == "2", "Replacement app did not relaunch"
            arguments = [arg for arg in result["arguments"][1:] if not arg.startswith("-psn_")]
            assert arguments == ["--skip-update", str(work.resolve())], f"Relaunch lost the requested directory: {result}"
            assert plistlib.loads((old / "Contents/Info.plist").read_bytes())["CFBundleVersion"] == "2"
            assert not shell_started.exists(), "A terminal opened before startup update finished"
            print("PASS: signed update installed and relaunched before any terminal opened", flush=True)
        finally:
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
            server.shutdown()
            subprocess.run(["defaults", "delete", bundle_id], capture_output=True)


if __name__ == "__main__":
    main()

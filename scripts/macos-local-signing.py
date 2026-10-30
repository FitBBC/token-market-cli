#!/usr/bin/env python3
"""Stable, certificate-pinned signatures for the macOS CLI/desktop test pair.

This is an internal-test identity, not Developer ID or a Gatekeeper bypass.
Private signing material stays on the build machine, outside the repository.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import secrets
import shlex
import shutil
import subprocess
import sys
import tempfile

APP_ID = "cn.tokenmarket.setup.test"
CLI_ID = "cn.tokenmarket.setup.cli.test"
IDENTITY_NAME = "Token Market Internal Test Signing"
ROOT = Path(__file__).resolve().parent.parent
DEFAULT_IDENTITY = Path.home() / "Library/Application Support/Token Market/Local Signing"


def run(args, *, label=None, input=None):
    result = subprocess.run([str(arg) for arg in args], input=input, capture_output=True)
    if result.returncode:
        # Never include arguments: some security/openssl operations take passwords.
        detail = ""
        if Path(args[0]).name in ("codesign", "csreq", "clang", "hdiutil"):
            detail = ": " + result.stderr.decode(errors="replace").strip()
        raise RuntimeError(f"{label or Path(args[0]).name} failed (exit {result.returncode}){detail}")
    return result.stdout


def requirement(fingerprint):
    if not re.fullmatch(r"[0-9A-F]{40}", fingerprint):
        raise ValueError("Invalid signing certificate SHA-1")
    return (f'certificate leaf = H"{fingerprint}" and '
            f'(identifier "{APP_ID}" or identifier "{CLI_ID}")')


def fingerprint(directory):
    data = run(["/usr/bin/openssl", "x509", "-in", directory / "certificate.pem", "-outform", "DER"])
    return hashlib.sha1(data).hexdigest().upper()


@contextlib.contextmanager
def identity(directory, *, initialize=False):
    directory = directory.expanduser().resolve()
    # Refuse a checked-in/generated signing key within this checkout.
    if directory == ROOT or ROOT in directory.parents:
        raise RuntimeError("Store signing material outside the repository")
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    directory.chmod(0o700)
    with (directory / "build.lock").open("a") as lock:
        os.chmod(lock.name, 0o600)
        fcntl.flock(lock, fcntl.LOCK_EX)
        keychain = directory / "signing.keychain-db"
        password_path = directory / "keychain-password"
        certificate = directory / "certificate.pem"
        ready = directory / "identity.json"
        if not ready.exists():
            if not initialize:
                raise RuntimeError("Signing identity missing; run the init command first")
            if keychain.exists() or password_path.exists() or certificate.exists():
                raise RuntimeError("Incomplete signing identity; preserve it and inspect before retrying")
            password = secrets.token_urlsafe(48)
            password_path.write_text(password)
            password_path.chmod(0o600)
            search_list = shlex.split(run(["/usr/bin/security", "list-keychains", "-d", "user"]).decode())
            try:
                run(["/usr/bin/security", "create-keychain", "-p", password, keychain], label="Create private signing keychain")
            finally:
                # create-keychain adds to the search list. Remove only our entry,
                # retaining other entries even if another app changed the list.
                current = shlex.split(run(["/usr/bin/security", "list-keychains", "-d", "user"]).decode())
                retained = [path for path in current if Path(path).resolve() != keychain]
                if retained != current:
                    run(["/usr/bin/security", "list-keychains", "-d", "user", "-s", *retained])
                if not retained and search_list:
                    raise RuntimeError("Unexpected empty keychain search list; inspect keychain preferences")
            with tempfile.TemporaryDirectory(prefix="identity-", dir=directory) as staging:
                staging = Path(staging)
                config = staging / "openssl.cnf"
                config.write_text("[req]\nprompt=no\ndistinguished_name=dn\nx509_extensions=extensions\n"
                                  f"[dn]\nCN={IDENTITY_NAME}\n"
                                  "[extensions]\nbasicConstraints=critical,CA:FALSE\n"
                                  "keyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\n")
                run(["/usr/bin/openssl", "req", "-new", "-newkey", "rsa:3072", "-nodes", "-x509",
                     "-sha256", "-days", "3650", "-config", config,
                     "-keyout", staging / "private.pem", "-out", certificate], label="Create local signing certificate")
                run(["/usr/bin/openssl", "pkcs12", "-export", "-inkey", staging / "private.pem",
                     "-in", certificate, "-out", staging / "identity.p12", "-passout", "stdin"],
                    input=(password + "\n").encode(), label="Prepare signing identity")
                run(["/usr/bin/security", "import", staging / "identity.p12", "-k", keychain,
                     "-P", password, "-T", "/usr/bin/codesign"], label="Import into private signing keychain")
                run(["/usr/bin/security", "set-key-partition-list", "-S", "apple-tool:",
                     "-s", "-k", password, keychain], label="Authorize local signing tool")
            ready.write_text(json.dumps({"certificateSha1": fingerprint(directory), "internalTestOnly": True}, indent=2) + "\n")
            ready.chmod(0o600)
        if not keychain.is_file() or not password_path.is_file() or not certificate.is_file():
            raise RuntimeError("Signing identity is incomplete; it was not regenerated")
        digest = fingerprint(directory)
        if json.loads(ready.read_text())["certificateSha1"] != digest:
            raise RuntimeError("Signing certificate changed; refusing to change client identity")
        password = password_path.read_text()
        run(["/usr/bin/security", "unlock-keychain", "-p", password, keychain], label="Unlock private signing keychain")
        try:
            yield keychain, digest
        finally:
            run(["/usr/bin/security", "lock-keychain", keychain], label="Lock private signing keychain")


def verify(path, digest):
    run(["/usr/bin/codesign", "--verify", "--strict", "-R", "=" + requirement(digest), path], label="Verify certificate-pinned client signature")
    output = run(["/usr/bin/codesign", "-d", "-r-", path]).decode().strip()
    # -R verifies membership; this additionally checks both programs declare the
    # same DR, which is what a newly created keychain item's ACL remembers.
    expected = run(["/usr/bin/csreq", "-r", "=" + requirement(digest), "-t"]).decode().strip()
    actual = output.removeprefix("designated => ")
    if actual != expected:
        raise RuntimeError("Client does not declare the shared designated requirement")


def sign(path, identifier, keychain, digest):
    run(["/usr/bin/codesign", "--force", "--sign", digest, "--keychain", keychain,
         "--timestamp=none", "--preserve-metadata=entitlements,flags,runtime",
         "--identifier", identifier, "--requirements",
         "=designated => " + requirement(digest), path], label="Sign internal-test client")
    verify(path, digest)


def package(args):
    source_app, source_cli = args.app.resolve(), args.cli.resolve()
    with (source_app / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info.get("CFBundleIdentifier") != APP_ID:
        raise RuntimeError("Only the test app can use the internal-test shared identity")
    if not source_cli.is_file():
        raise RuntimeError("CLI executable missing")
    for binary in (source_cli, source_app / "Contents/MacOS" / info["CFBundleExecutable"]):
        if run(["/usr/bin/lipo", "-archs", binary]).decode().strip() != "arm64":
            raise RuntimeError("This packager expects macOS arm64 clients")
    cli_version = run([source_cli, "--version"]).decode().strip().split()
    if len(cli_version) != 2 or cli_version[0] != "tokenmarket":
        raise RuntimeError("Unexpected CLI version response")
    app_version = info["CFBundleShortVersionString"]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise RuntimeError("Output directory must be empty; existing artifacts were preserved")
    app = output / source_app.name
    cli = output / f"tokenmarket-test-{cli_version[1]}-macos-arm64"
    run(["/usr/bin/ditto", source_app, app])
    shutil.copy2(source_cli, cli)
    with identity(args.identity_dir) as (keychain, digest):
        sign(app, APP_ID, keychain, digest)
        sign(cli, CLI_ID, keychain, digest)
    dmg = output / f"TokenMarket-Test-{app_version}-arm64.dmg"
    with tempfile.TemporaryDirectory(prefix="tokenmarket-dmg-") as staging:
        run(["/usr/bin/ditto", app, Path(staging) / app.name])
        (Path(staging) / "Applications").symlink_to("/Applications")
        run(["/usr/bin/hdiutil", "create", "-volname", "Token Market Test", "-srcfolder", staging,
             "-format", "UDZO", dmg], label="Create signed-test DMG")
    run(["/usr/bin/hdiutil", "verify", dmg], label="Verify DMG integrity")
    manifest = {"internalTestOnly": True, "certificateSha1": digest,
                "sourceApp": str(source_app), "sourceCli": str(source_cli),
                "sharedRequirement": requirement(digest),
                "artifacts": {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in (cli, dmg)}}
    (output / "signing-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Signed test pair and DMG: {output}")


def smoke(args):
    # Dummy keychain, dummy passwords and no UI. Real login/tool credentials are
    # never read. A missing ACL authorization must fail rather than display a UI.
    with tempfile.TemporaryDirectory(prefix="tokenmarket-signing-smoke-") as temporary:
        temporary = Path(temporary)
        first, second = temporary / "desktop", temporary / "cli"
        source = ROOT / "tests/helpers/macos-keychain-sharing.c"
        for binary, variant in ((first, "1"), (second, "2")):
            run(["/usr/bin/clang", "-Wno-deprecated-declarations", "-DVARIANT=" + variant,
                 source, "-framework", "Security", "-framework", "CoreFoundation", "-o", binary])
        keychain_path = temporary / "fixture.keychain-db"
        with identity(args.identity_dir) as (keychain, digest):
            sign(first, APP_ID, keychain, digest)
            sign(second, CLI_ID, keychain, digest)
            # Each command starts a fresh process. The helper selects only its
            # private keychain and disables all keychain UI before any operation.
            for binary, operation in ((first, "create"), (second, "read"), (second, "update"), (first, "read-updated")):
                run([binary, operation, keychain_path], label=f"Cross-process keychain {operation}")
            moved = temporary / "moved-cli"
            shutil.copy2(second, moved)
            run([moved, "read-updated", keychain_path], label="Read after executable relocation")
            run(["/usr/bin/clang", "-Wno-deprecated-declarations", "-DVARIANT=3", source,
                 "-framework", "Security", "-framework", "CoreFoundation", "-o", second])
            sign(second, CLI_ID, keychain, digest)
            run([second, "read-updated", keychain_path], label="Read after signed executable update")
            run(["/usr/bin/codesign", "--force", "--sign", "-", moved])
            unauthorized = subprocess.run([str(moved), "read-updated", str(keychain_path)], capture_output=True)
            if unauthorized.returncode != 10:
                raise RuntimeError("Unauthorized executable was not rejected by the keychain ACL")
            run(["/usr/bin/codesign", "--force", "--sign", digest, "--keychain", keychain,
                 "--timestamp=none", "--identifier", "cn.tokenmarket.unrelated", moved])
            unrelated = subprocess.run([str(moved), "read-updated", str(keychain_path)], capture_output=True)
            if unrelated.returncode != 10:
                raise RuntimeError("Same-certificate unrelated executable was not rejected")
            run([first, "delete", keychain_path], label="Delete isolated fixture keychain")
    print("Passed: bidirectional access, process restart, relocation, rebuild, and rejection of ad-hoc/unrelated readers; keychain UI disabled throughout.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--identity-dir", type=Path, default=DEFAULT_IDENTITY)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("init", help="Create once; subsequent builds reuse the same private signing identity")
    commands.add_parser("smoke", help="Exercise dummy cross-process keychain access with UI disabled")
    package_parser = commands.add_parser("package", help="Copy and sign an existing test app/CLI, then create a DMG")
    package_parser.add_argument("--app", type=Path, required=True)
    package_parser.add_argument("--cli", type=Path, required=True)
    package_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("Local test signing requires macOS")
    os.umask(0o077)
    if args.command == "init":
        with identity(args.identity_dir, initialize=True) as (_, digest):
            print(f"Stable internal-test certificate: {digest}")
    elif args.command == "smoke":
        smoke(args)
    else:
        package(args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)

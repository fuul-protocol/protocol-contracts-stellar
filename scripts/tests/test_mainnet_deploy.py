"""Client deployment checks using isolated fake tools, never a real network."""

import json
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
NATIVE, CURRENCY, MANAGER, FACTORY, PROJECT = ("C" + c * 55 for c in "NCMFP")
HASHES = {name: c * 64 for name, c in (("project", "a"), ("manager", "b"), ("factory", "c"))}

FAKE = r'''
import json, os, pathlib, sys
tool = pathlib.Path(sys.argv[0]).name
a = sys.argv[1:]
root = pathlib.Path.cwd()
record = root / ".keys/mainnet/deployment.env"
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"tool": tool, "args": a,
                          "record": record.read_text() if record.exists() else None}) + "\n")
def val(flag): return a[a.index(flag) + 1]
def out(value): print(value); sys.exit(0)
def fail(): print("fake failure: SECRET_SENTINEL", file=sys.stderr); sys.exit(9)
if os.environ.get("FAIL_COMMAND") == " ".join([tool] + a[:3]): fail()
hashes = {"project": "a" * 64, "manager": "b" * 64, "factory": "c" * 64}
ids = {"manager": "C" + "M" * 55, "factory": "C" + "F" * 55,
       "project": "C" + "P" * 55}
mode = os.environ.get("BAD_RESULT", "")
if tool == "rustc" and a == ["--version"]: out("rustc 1.92.0 (fake)")
if tool == "rustup" and a == ["target", "list", "--installed"]: out("wasm32v1-none")
if tool == "cargo" and a in (["--version"], ["test", "--workspace", "--locked"]):
    out("cargo 1.92.0 (fake)")
if tool != "stellar": fail()
if a == ["--version"]: out(os.environ.get("CLI_VERSION", "stellar 27.1.0 (fake)"))
if a[:2] == ["keys", "generate"]:
    config = pathlib.Path(val("--config-dir"))
    assert config.parts[-2:] == ("mainnet", "stellar")
    (config / "identity").mkdir(parents=True, exist_ok=True)
    (config / "identity" / (a[2] + ".toml")).write_text("fake identity")
    out("")
if a[:2] in (["keys", "address"], ["keys", "secret"]):
    letter = "ABCDEFGHIJKLMNOPQRST"[int(a[2].split("-")[1]) - 1]
    out("G" + letter * 55 if a[1] == "address" else "SECRET_SENTINEL")
if a == ["contract", "build", "--locked"]:
    target = root / "target/wasm32v1-none/release"
    target.mkdir(parents=True, exist_ok=True)
    for name in ["project", "manager", "factory"]:
        (target / ("fuul_" + name + ".wasm")).write_bytes(b"fake wasm")
    out("")
if a[:2] == ["network", "add"]:
    assert a[2] == "fuul"
    assert val("--config-dir") == str(root / ".keys/mainnet/stellar")
    out("")
if a[:2] == ["keys", "fund"] or a[:3] == ["contract", "asset", "deploy"]:
    fail()
if a[:4] == ["ledger", "entry", "fetch", "account"]:
    if os.environ.get("UNFUNDED"): fail()
    out('{"account":"fake funded account"}')
if a[:3] == ["contract", "id", "asset"]: out("C" + "N" * 55)
if a[:3] == ["contract", "info", "hash"]:
    if "--wasm" in a:
        out(hashes[pathlib.Path(val("--wasm")).stem.removeprefix("fuul_")])
    name = next(k for k, v in ids.items() if v == val("--id"))
    out("d" * 64 if mode == "deployed_hash" else hashes[name])
if a[:2] == ["contract", "upload"]:
    assert "--optimize=false" in a
    name = pathlib.Path(val("--wasm")).stem.removeprefix("fuul_")
    out("oops" if mode == "malformed_hash" else
        "d" * 64 if mode == "upload_hash" else hashes[name])
if a[:2] == ["contract", "deploy"]:
    assert "--" in a
    name = next(k for k, v in hashes.items() if v == val("--wasm-hash"))
    out("invalid" if mode == "address" else ids[name])
if a[:2] == ["contract", "invoke"]:
    method = a[a.index("--") + 1]
    if method == "create_fuul_project":
        assert "--send=yes" in a
        out(json.dumps(ids["project"]))
    assert "--send=no" in a
    if method == "factory": out(json.dumps(ids["manager"] if mode == "factory" else ids["factory"]))
    if method == "required_signers": out('"2"' if mode == "quorum" else '"1"')
    if method == "has_manager_role": out("false" if mode == "manager_role" else "true")
fail()
'''


class MainnetDeploymentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="fuul mainnet mock ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "scripts").mkdir()
        for name in ("keys.sh", "deploy.sh", "status.sh"):
            source = ROOT / "scripts" / name
            if source.exists():
                shutil.copyfile(source, self.root / "scripts" / name)
        manifest = self.root / "contracts/fuul-manager/Cargo.toml"
        manifest.parent.mkdir(parents=True)
        manifest.write_text("# isolated fixture\n")
        subprocess.run([shutil.which("git"), "init", "-q", str(self.root)], check=True)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("stellar", "cargo", "rustc", "rustup"):
            tool = self.bin / name
            tool.write_text(f"#!{sys.executable}\n" + FAKE)
            tool.chmod(0o700)
        self.env = {"PATH": f"{self.bin}:/usr/bin:/bin", "HOME": str(self.root),
                    "FAKE_LOG": str(self.root / "commands.jsonl"), "NO_COLOR": "1"}

    def invoke(self, script, *args, **env):
        return subprocess.run(["/bin/bash", f"scripts/{script}.sh", *args],
                              cwd=self.root, env={**self.env, **env}, text=True,
                              capture_output=True)

    def prepare(self, *args, **env):
        return self.invoke("deploy", "--prepare-keys", *args, **env)

    def deploy(self, *args, **env):
        return self.invoke("deploy", "--project-uri", "ipfs://client project",
                           "--rpc-url", "https://client-rpc.invalid",
                           "--currency", CURRENCY,
                           "--currency-limit", "1000000000", *args, **env)

    def record(self):
        return self.root / ".keys/mainnet/deployment.env"

    def calls(self, *prefix, tool="stellar"):
        log = self.root / "commands.jsonl"
        entries = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        return [entry for entry in entries if entry["tool"] == tool
                and entry["args"][:len(prefix)] == list(prefix)]

    def assert_failed(self, result):
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("SECRET_SENTINEL", result.stdout + result.stderr)

    def test_help_and_mainnet_key_preparation_do_not_touch_the_network(self):
        help_result = self.invoke("deploy", "--help")
        self.assertEqual(help_result.returncode, 0, help_result.stderr)
        self.assertIn("--prepare-keys", help_result.stdout)
        self.assertFalse((self.root / ".keys").exists())
        first = self.prepare()
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        self.assertEqual(len(self.calls("keys", "generate")), 7)
        self.assertFalse(self.record().exists())
        self.assertFalse(self.calls("network", "add"))
        self.assertFalse(self.calls("ledger", "entry", "fetch", "account"))
        self.assertFalse(self.calls("contract", "upload"))
        self.assertIn("[ok] Created 7 Mainnet identities", first.stdout)
        self.assertIn("G" + "A" * 55, first.stdout)
        self.assertIn(".keys/mainnet/keys.env", first.stdout)
        self.assertIn("Fund these public addresses", first.stdout)
        keys = self.root / ".keys/mainnet/keys.env"
        self.assertEqual(stat.S_IMODE(keys.stat().st_mode), 0o600)
        before = keys.read_bytes()
        count = len(self.calls("keys", "generate"))
        second = self.prepare()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertIn("[ok] Loaded 7 Mainnet identities", second.stdout)
        self.assertEqual(len(self.calls("keys", "generate")), count)
        self.assertEqual(keys.read_bytes(), before)

    def test_deployment_refuses_missing_keys_and_unfunded_accounts(self):
        missing = self.deploy()
        self.assert_failed(missing)
        self.assertIn("--prepare-keys", missing.stderr)
        self.assertFalse(self.calls("keys", "generate"))
        self.assertEqual(self.prepare().returncode, 0)
        unfunded = self.deploy(UNFUNDED="1")
        self.assert_failed(unfunded)
        self.assertIn("funded", unfunded.stderr)
        self.assertFalse(self.calls("contract", "upload"))
        self.assertFalse(self.calls("contract", "build"))
        self.assertFalse(self.record().exists())
        self.assertIn("rerun", unfunded.stderr.lower())
        funded = self.deploy()
        self.assertEqual(funded.returncode, 0, funded.stdout + funded.stderr)

    def test_funded_mainnet_deployment_records_and_verifies_outputs(self):
        self.assertEqual(self.prepare().returncode, 0)
        keys_before = (self.root / ".keys/mainnet/keys.env").read_bytes()
        (self.root / "commands.jsonl").unlink()
        result = self.deploy()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.calls("keys", "generate"))
        self.assertFalse(self.calls("keys", "fund"))
        self.assertFalse(self.calls("contract", "asset", "deploy"))
        network = self.calls("network", "add")[0]["args"]
        self.assertEqual(network[network.index("--network-passphrase") + 1],
                         "Public Global Stellar Network ; September 2015")
        self.assertEqual(len(self.calls("ledger", "entry", "fetch", "account")), 7)
        self.assertTrue(all(call["record"] is None for call in self.calls("ledger", "entry", "fetch", "account")))
        self.assertTrue(all(call["record"] is None for call in self.calls("contract", "build")))
        self.assertEqual(len(self.calls("contract", "upload")), 3)
        self.assertEqual(len(self.calls("contract", "deploy")), 2)
        self.assertEqual(len(self.calls("contract", "info", "hash")), 6)
        self.assertTrue(all("in_progress" in entry["record"] for entry in self.calls("contract", "upload")))
        invokes = [entry["args"] for entry in self.calls("contract", "invoke")]
        self.assertIn("--send=yes", invokes[0])
        self.assertTrue(all("--send=no" in args for args in invokes[1:]))
        manager = self.calls("contract", "deploy")[0]["args"]
        self.assertEqual(manager[manager.index("--source") + 1], "fuul-1")
        self.assertEqual(manager[manager.index("--network") + 1], "fuul")
        self.assertEqual(json.loads(manager[manager.index("--claim_signers") + 1]), ["G" + "D" * 55])
        self.assertEqual(manager[manager.index("--accepted_currency") + 1], CURRENCY)
        self.assertIn("PROJECT_HASH=" + HASHES["project"], result.stdout)
        self.assertIn("[ok] Loaded 7 Mainnet identities", result.stdout)
        self.assertIn("[ok] Deployed code hashes verified", result.stdout)
        self.assertNotIn("SECRET_SENTINEL", result.stdout + result.stderr)
        self.assertNotIn("export ADMIN_SECRET", result.stdout)
        self.assertEqual(keys_before, (self.root / ".keys/mainnet/keys.env").read_bytes())
        self.assertIn("complete", self.record().read_text())

    def test_failed_write_keeps_partial_record_and_does_not_repeat(self):
        self.assertEqual(self.prepare().returncode, 0)
        result = self.deploy(FAIL_COMMAND="stellar contract deploy --wasm-hash")
        self.assert_failed(result)
        self.assertEqual(len(self.calls("contract", "deploy")), 1)
        self.assertFalse(self.calls("contract", "invoke"))
        self.assertIn("in_progress", self.record().read_text())
        self.assertIn("deploy_manager", self.record().read_text())
        self.assertTrue(any("SECRET_SENTINEL" in log.read_text()
                            for log in (self.root / ".keys").glob("deploy-mainnet.log.*")))
        before = self.record().read_bytes()
        count = len(self.calls())
        self.assert_failed(self.deploy())
        self.assertEqual(self.record().read_bytes(), before)
        self.assertEqual(len(self.calls()), count)

    def test_build_failure_before_first_upload_can_be_retried(self):
        self.assertEqual(self.prepare().returncode, 0)
        failed = self.deploy(FAIL_COMMAND="stellar contract build --locked")
        self.assert_failed(failed)
        self.assertFalse(self.record().exists())
        self.assertFalse(self.calls("contract", "upload"))
        self.assertIn("rerun", failed.stderr.lower())
        retried = self.deploy()
        self.assertEqual(retried.returncode, 0, retried.stdout + retried.stderr)

    def test_invalid_mainnet_inputs_fail_before_key_generation_or_network(self):
        for args in ((), ("--network", "testnet"), ("--currency", "invalid"),
                     ("--currency-limit", "0"), ("--currency-limit", str(2**256))):
            with self.subTest(args=args):
                result = self.invoke("deploy", "--project-uri", "ipfs://project", *args)
                self.assert_failed(result)
                self.assertFalse((self.root / ".keys").exists())
        self.assert_failed(self.prepare("--network", "testnet"))
        self.assertFalse(self.calls("keys", "generate"))

    def test_status_reads_local_mainnet_record_then_verifies_without_writes(self):
        self.assertEqual(self.prepare().returncode, 0)
        self.assertEqual(self.deploy().returncode, 0)
        (self.root / "commands.jsonl").unlink()
        offline = self.invoke("status")
        self.assertEqual(offline.returncode, 0, offline.stderr)
        self.assertIn("Status: complete", offline.stdout)
        self.assertIn("Network: mainnet", offline.stdout)
        self.assertFalse((self.root / "commands.jsonl").exists())
        online = self.invoke("status", "--verify")
        self.assertEqual(online.returncode, 0, online.stdout + online.stderr)
        self.assertEqual(len(self.calls("contract", "info", "hash")), 3)
        self.assertEqual(len(self.calls("contract", "invoke")), 3)
        self.assertTrue(all("--send=no" in entry["args"] for entry in self.calls("contract", "invoke")))
        self.assertFalse(self.calls("contract", "deploy"))

    def test_preparation_failure_keeps_diagnostics_without_partial_deployment(self):
        result = self.prepare(FAIL_COMMAND="stellar keys generate fuul-1")
        self.assert_failed(result)
        self.assertFalse(self.record().exists())
        self.assertFalse(self.calls("contract", "upload"))
        self.assertNotIn("SECRET_SENTINEL", result.stdout + result.stderr)
        logs = list((self.root / ".keys").glob("deploy-mainnet.log.*"))
        self.assertEqual(len(logs), 1)
        self.assertEqual(stat.S_IMODE(logs[0].stat().st_mode), 0o600)
        self.assertIn("SECRET_SENTINEL", logs[0].read_text())

    def test_existing_record_and_untrusted_status_data_never_run_code(self):
        self.assertEqual(self.prepare().returncode, 0)
        self.assertEqual(self.deploy().returncode, 0)
        before = self.record().read_bytes()
        self.assert_failed(self.deploy())
        self.assertEqual(self.record().read_bytes(), before)
        self.record().write_text(before.decode().replace("export MANAGER=" + MANAGER,
                                                      "export MANAGER=$(touch INJECTED)"))
        (self.root / "commands.jsonl").unlink()
        self.assert_failed(self.invoke("status"))
        self.assertFalse((self.root / "INJECTED").exists())
        self.assertFalse((self.root / "commands.jsonl").exists())

    def test_full_u256_limit_and_invalid_cli_results(self):
        self.assertEqual(self.prepare().returncode, 0)
        limit = str(2**256 - 1)
        result = self.deploy("--currency-limit", limit)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        manager = self.calls("contract", "deploy")[0]["args"]
        self.assertEqual(manager[manager.index("--initial_currency_limit") + 1], limit)
        self.assertIn("export INITIAL_CURRENCY_LIMIT=" + limit, self.record().read_text())

        other = MainnetDeploymentTests()
        other.setUp()
        try:
            self.assertEqual(other.prepare().returncode, 0)
            bad = other.deploy(BAD_RESULT="upload_hash")
            other.assert_failed(bad)
            self.assertFalse(other.calls("contract", "deploy"))
            self.assertIn("in_progress", other.record().read_text())
        finally:
            other.doCleanups()

    def test_shared_roles_and_quoted_project_uri_remain_literal(self):
        self.assertEqual(self.prepare("--key-count", "1").returncode, 0)
        uri = 'ipfs://client space/"quote"; $(touch INJECTED)'
        result = self.deploy("--project-uri", uri)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse((self.root / "INJECTED").exists())
        manager = self.calls("contract", "deploy")[0]["args"]
        self.assertEqual(json.loads(manager[manager.index("--claim_signers") + 1]), ["G" + "A" * 55])
        invoked = self.calls("contract", "invoke")[0]["args"]
        self.assertEqual(invoked[invoked.index("--project_info_uri") + 1], uri)
        self.assertEqual(len(self.calls("ledger", "entry", "fetch", "account")), 1)

    def test_bad_results_and_readbacks_never_mark_deployment_complete(self):
        for mode in ("malformed_hash", "upload_hash", "address", "deployed_hash",
                     "factory", "quorum", "manager_role"):
            with self.subTest(mode=mode):
                case = MainnetDeploymentTests()
                case.setUp()
                try:
                    self.assertEqual(case.prepare().returncode, 0)
                    result = case.deploy(BAD_RESULT=mode)
                    case.assert_failed(result)
                    self.assertIn("in_progress", case.record().read_text())
                    self.assertNotIn("Deployment complete", result.stdout)
                    if mode in ("malformed_hash", "upload_hash"):
                        self.assertFalse(case.calls("contract", "deploy"))
                finally:
                    case.doCleanups()

    def test_invalid_cli_version_xtrace_and_symlink_are_rejected_before_keys(self):
        wrong_version = self.prepare(CLI_VERSION="stellar 26.0.0 (fake)")
        self.assert_failed(wrong_version)
        self.assertFalse((self.root / ".keys").exists())
        traced = subprocess.run(["/bin/bash", "-x", "scripts/deploy.sh", "--prepare-keys"],
                                cwd=self.root, env=self.env, capture_output=True, text=True)
        self.assert_failed(traced)
        self.assertFalse(self.calls("keys", "generate"))
        self.assertFalse((self.root / ".keys").exists())
        self.record().parent.mkdir(parents=True)
        self.record().symlink_to(self.root / "missing")
        blocked = self.deploy()
        self.assert_failed(blocked)
        self.assertTrue(self.record().is_symlink())
        self.assertFalse(self.calls("network", "add"))

    def test_incomplete_status_refuses_network_verification(self):
        self.assertEqual(self.prepare().returncode, 0)
        self.assert_failed(self.deploy(FAIL_COMMAND="stellar contract deploy --wasm-hash"))
        (self.root / "commands.jsonl").unlink()
        status = self.invoke("status")
        self.assertEqual(status.returncode, 0, status.stderr)
        self.assertIn("Status: in_progress", status.stdout)
        self.assertIn("deploy_manager", status.stdout)
        self.assert_failed(self.invoke("status", "--verify"))
        self.assertFalse((self.root / "commands.jsonl").exists())


if __name__ == "__main__":
    unittest.main()

#!/usr/bin/env python3
"""Negative controls for the SongCore build/package provenance boundary.

Every control executes the real gate and asserts the real diagnostic: the
shared receipt verifier, the shared version-identity verifier, or the
production packaging entry itself. Two structural assertions remain, each
labelled with why no executable local oracle exists (running the PowerShell
build entry needs WSL plus a derived FFmpeg closure; the Xmake gate needs
xmake), so a regression is still visible on a plain checkout.
"""
from __future__ import annotations
import hashlib, json, os, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERIFY = ROOT / "tools" / "verify_songcore_build_receipt.py"
VERIFY_VERSION = ROOT / "tools" / "verify_songcore_version_identity.py"

def sha256(path: Path) -> str: return hashlib.sha256(path.read_bytes()).hexdigest()
def write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)

def run_verify(root: Path, receipt: Path, manifest: Path | None = None):
    argv = [sys.executable, str(VERIFY), "--receipt", str(receipt), "--source-commit", "a"*40,
            "--artifact", str(root/"libsongcore.a"), "--header", str(root/"songcore.h"),
            "--recipe", str(root/"target.json"), "--profile", str(root/"profile.json"),
            "--pin", str(root/"pin.json"), "--closure", str(root/"closure.json"),
            "--artifact-shared", str(root/"libsongcore.so"), "--target", "windows-x86_64",
            "--toolchain", "gcc-13+xmake-2.9.7", "--source-tree", "b"*40]
    if manifest: argv += ["--package-manifest", str(manifest)]
    return subprocess.run(argv, text=True, capture_output=True, check=False)

def baseline(root: Path):
    files = {n: root/f for n, f in {"artifact":"libsongcore.a", "shared":"libsongcore.so", "header":"songcore.h", "recipe":"target.json", "profile":"profile.json", "pin":"pin.json", "closure":"closure.json"}.items()}
    for name, path in files.items(): write(path, (name+"\n").encode())
    receipt = {"schema_version":1, "qualification":"RELEASE_QUALIFIED",
      "source":{"commit":"a"*40,"tree_oid":"b"*40},
      "target":{"id":"windows-x86_64"}, "toolchain":{"identity":"gcc-13+xmake-2.9.7"},
      "inputs":{name:{"sha256":sha256(path)} for name,path in files.items() if name not in ("artifact","shared")},
      "artifacts":{"static":{"sha256":sha256(files["artifact"]),"bytes":files["artifact"].stat().st_size},
                   "shared":{"sha256":sha256(files["shared"]),"bytes":files["shared"].stat().st_size}},
      "execution":{"compile_actions":1,"cache_reuse":False}}
    path=root/"receipt.json"; path.write_text(json.dumps(receipt),encoding="utf-8"); return path,receipt

def expect_red(name, result, diagnostic):
    output=result.stdout+result.stderr
    if result.returncode == 0 or diagnostic not in output:
        raise AssertionError(f"{name}: expected RED containing {diagnostic!r}; rc={result.returncode}; output={output}")

def mutate(root: Path, receipt: dict, dotted: str, value):
    mutated=json.loads(json.dumps(receipt)); cursor=mutated; parts=dotted.split(".")
    for part in parts[:-1]: cursor=cursor[part]
    cursor[parts[-1]]=value
    path=root/f"{dotted}.json"; path.write_text(json.dumps(mutated),encoding="utf-8"); return path

def git_fixture(root: Path) -> str:
    subprocess.run(["git","init","-q",str(root)],check=True)
    subprocess.run(["git","-C",str(root),"config","user.email","fixture@example.invalid"],check=True)
    subprocess.run(["git","-C",str(root),"config","user.name","fixture"],check=True)
    write(root/"README",b"fixture\n"); subprocess.run(["git","-C",str(root),"add","README"],check=True)
    subprocess.run(["git","-C",str(root),"commit","-qm","fixture"],check=True)
    return subprocess.check_output(["git","-C",str(root),"rev-parse","HEAD"],text=True).strip()

def production_entry(repo: Path, *args):
    return subprocess.run(["bash",str(ROOT/"ops/package-linux.sh"),*args],
                          text=True,capture_output=True,check=False)

def main() -> int:
    if not VERIFY.is_file(): raise AssertionError(f"missing provenance verifier: {VERIFY}")
    with tempfile.TemporaryDirectory(prefix="songcore-provenance-") as td:
        root=Path(td); receipt_path,receipt=baseline(root)
        green=run_verify(root,receipt_path)
        if green.returncode: raise AssertionError(f"baseline must be GREEN: {green.stdout}{green.stderr}")
        mutations=[("M1","source.commit","c"*40,"source.commit"),("M2","artifacts.static.sha256","d"*64,"artifact sha256"),
          ("M4","inputs.header.sha256","e"*64,"header sha256"),("M5","inputs.recipe.sha256","f"*64,"recipe sha256"),
          ("M6","toolchain.identity","clang-99","toolchain identity"),("M1-tree","source.tree_oid","0"*40,"source tree identity"),
          ("M9","execution.cache_reuse",True,"no cache reuse"),
          ("M10","artifacts.static.bytes",999,"artifact bytes")]
        for name,dotted,value,diagnostic in mutations:
            expect_red(name,run_verify(root,mutate(root,receipt,dotted,value)),diagnostic)
        write(root/"libsongcore.a",b"old archive\n"); expect_red("M3",run_verify(root,receipt_path),"artifact sha256"); write(root/"libsongcore.a",b"artifact\n")
        expect_red("M8",run_verify(root,root/"missing.json"),"receipt missing")
        # M11: the header digest absent from the receipt (the a5bacb6 defect class:
        # a build receipt that never hashed native/include/songcore.h).
        absent=json.loads(json.dumps(receipt)); del absent["inputs"]["header"]
        path=root/"M11.json"; path.write_text(json.dumps(absent),encoding="utf-8")
        expect_red("M11",run_verify(root,path),"receipt missing inputs.header")
        manifest=root/"manifest.json"; manifest.write_text(json.dumps({"source":{"commit":"0"*40}}),encoding="utf-8")
        expect_red("M7",run_verify(root,receipt_path,manifest),"manifest source.commit")
    if os.name != "nt":
        with tempfile.TemporaryDirectory(prefix="songcore-package-entry-") as td:
            repo=Path(td); commit=git_fixture(repo)
            expect_red("M8-production-entry",
                production_entry(repo,"--version","1.2.3","--source-commit",commit,
                    "--work-root",str(repo),"--out-dir",str(repo/"out"),
                    "--build-receipt",str(repo/"missing.json")),"missing build receipt")
            # Executed version-grammar and immutability gates of the real entry.
            expect_red("IMMUTABLE-HISTORY",
                production_entry(repo,"--version","0.1.0","--source-commit",commit,
                    "--work-root",str(repo),"--out-dir",str(repo/"out")),"immutable history")
            expect_red("VERSION-SUFFIX",
                production_entry(repo,"--version","0.1.1-rc.1","--source-commit",commit,
                    "--work-root",str(repo),"--out-dir",str(repo/"out")),"not the authorized MAJOR.MINOR.PATCH")
            expect_red("VERSION-LEADING-ZERO",
                production_entry(repo,"--version","01.2.3","--source-commit",commit,
                    "--work-root",str(repo),"--out-dir",str(repo/"out")),"not the authorized MAJOR.MINOR.PATCH")
    with tempfile.TemporaryDirectory(prefix="songcore-version-") as td:
        root=Path(td); xmake=root/"xmake.lua"; xmake.write_text('set_version("0.1.0")\n',encoding="utf-8")
        approval={"schema_version":1,"decision":"Q1","approved_by":"jnhu76",
          "authority_ref":"https://github.com/jnhu76/qianqian/issues/333#issuecomment-123",
          "semantic_version":"0.1.1","component_version_mode":"independent"}
        approval_path=root/"approval.json"; approval_path.write_text(json.dumps(approval),encoding="utf-8")
        decision={"schema_version":1,"status":"APPROVED",
          "authority_ref":"https://github.com/jnhu76/qianqian/issues/333#issuecomment-123",
          "authority_record_path":"approval.json",
          "authority_record_sha256":sha256(approval_path),"semantic_version":"0.1.1",
          "component_tag":"songcore-v0.1.1","manifest_version":"0.1.1",
          "linux_package_root":"songcore-v0.1.1-linux-x86_64",
          "windows_package_root":"songcore-v0.1.1-windows-x86_64",
          "component_version_mode":"independent","component_version":"0.1.1",
          "xmake_project_version":"0.1.0"}
        path=root/"docs"/"release"/"decision.json"; path.parent.mkdir(parents=True)
        def version_run(d, version="0.1.1"):
            path.write_text(json.dumps(d),encoding="utf-8")
            return subprocess.run([sys.executable,str(VERIFY_VERSION),"--decision",str(path),
              "--version",version,"--target","linux-x86_64","--xmake",str(xmake)],
              text=True,capture_output=True,check=False)
        green=version_run(decision)
        if green.returncode != 0: raise AssertionError("approved independent version fixture did not GREEN: "+green.stderr)
        blocked=dict(decision,status="OWNER_BLOCKED",semantic_version=None)
        expect_red("VERSION-BLOCKED",version_run(blocked),"OWNER_BLOCKED")
        no_authority=dict(decision,authority_ref=None)
        expect_red("VERSION-AUTHORITY",version_run(no_authority),"authority_ref")
        bad_digest=dict(decision,authority_record_sha256="0"*64)
        expect_red("VERSION-AUTHORITY-DIGEST",version_run(bad_digest),"digest mismatch")
        mismatch=dict(decision,windows_package_root="songcore-v9.9.9-windows-x86_64")
        expect_red("VERSION-MAPPING",version_run(mismatch),"windows_package_root")
        xmake_lie=dict(decision,xmake_project_version="9.9.9")
        expect_red("VERSION-XMAKE",version_run(xmake_lie),"native/xmake.lua")
        coupled=dict(decision,component_version_mode="project-coupled")
        expect_red("VERSION-COUPLING",version_run(coupled),"project-coupled")
    # ---- structural controls (no executable local oracle exists) --------------
    # The PowerShell build entry needs WSL plus an imported FFmpeg closure to run,
    # so the observed real defect (configure MinGW, then "xmake clean -a" erased
    # that configuration and the build silently fell back to Linux) is pinned as
    # statement order instead of a run.
    build_windows=(ROOT/"ops/build-rc.ps1").read_text(encoding="utf-8")
    clean = 'xmake clean -a > "$EV/clean.log" 2>&1'
    configure = 'xmake f -p mingw -a x86_64 -m release -y > "$EV/config.log" 2>&1'
    if build_windows.index(clean) > build_windows.index(configure):
        raise AssertionError("Windows build config is erased by a later xmake clean -a")
    # The Xmake closure gate needs xmake + a derived closure to evaluate; the
    # real builds exercise it, and CI triggers on native/build/**.
    ffmpeg=(ROOT/"native/build/ffmpeg.lua").read_text(encoding="utf-8")
    if "mt.recipe_sha256 ~= recipe_sha256" not in ffmpeg:
        raise AssertionError("native/build/ffmpeg.lua no longer rejects a stale target recipe")
    # CI trigger coverage cannot be executed locally at all: one compact guard
    # that the substrate workflow is wired to the files these controls read.
    workflow=(ROOT/".github/workflows/songcore-release-substrate.yml").read_text(encoding="utf-8")
    for trigger in ("ops/**","tools/verify_songcore_build_receipt.py",
                    "tools/verify_songcore_version_identity.py",
                    "tools/test_songcore_release_substrate.py"):
        if trigger not in workflow: raise AssertionError(f"workflow trigger missing: {trigger}")
    print("PASS: SongCore release substrate negative controls M1-M11 + version identity"); return 0

if __name__ == "__main__": raise SystemExit(main())

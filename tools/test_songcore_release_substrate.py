#!/usr/bin/env python3
"""Negative controls for the SongCore build/package provenance boundary."""
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
            mutated=json.loads(json.dumps(receipt)); cursor=mutated; parts=dotted.split(".")
            for part in parts[:-1]: cursor=cursor[part]
            cursor[parts[-1]]=value; path=root/f"{name}.json"; path.write_text(json.dumps(mutated),encoding="utf-8")
            expect_red(name,run_verify(root,path),diagnostic)
        write(root/"libsongcore.a",b"old archive\n"); expect_red("M3",run_verify(root,receipt_path),"artifact sha256"); write(root/"libsongcore.a",b"artifact\n")
        expect_red("M8",run_verify(root,root/"missing.json"),"receipt missing")
        manifest=root/"manifest.json"; manifest.write_text(json.dumps({"source":{"commit":"0"*40}}),encoding="utf-8")
        expect_red("M7",run_verify(root,receipt_path,manifest),"manifest source.commit")
    if os.name != "nt":
        with tempfile.TemporaryDirectory(prefix="songcore-package-entry-") as td:
            repo=Path(td); subprocess.run(["git","init","-q",str(repo)],check=True)
            subprocess.run(["git","-C",str(repo),"config","user.email","fixture@example.invalid"],check=True)
            subprocess.run(["git","-C",str(repo),"config","user.name","fixture"],check=True)
            write(repo/"README",b"fixture\n"); subprocess.run(["git","-C",str(repo),"add","README"],check=True)
            subprocess.run(["git","-C",str(repo),"commit","-qm","fixture"],check=True)
            commit=subprocess.check_output(["git","-C",str(repo),"rev-parse","HEAD"],text=True).strip()
            result=subprocess.run(["bash",str(ROOT/"ops/package-linux.sh"),"--version","1.2.3",
                "--source-commit",commit,"--work-root",str(repo),"--out-dir",str(repo/"out"),
                "--build-receipt",str(repo/"missing.json")],text=True,capture_output=True,check=False)
            expect_red("M8-production-entry",result,"missing build receipt")
    with tempfile.TemporaryDirectory(prefix="songcore-version-") as td:
        root=Path(td); xmake=root/"xmake.lua"; xmake.write_text('set_version("0.1.0")\n',encoding="utf-8")
        decision={"schema_version":1,"status":"APPROVED",
          "authority_ref":"https://github.com/jnhu76/qianqian/issues/333#issuecomment-123",
          "authority_record_sha256":"1"*64,"semantic_version":"0.1.1",
          "component_tag":"songcore-v0.1.1","manifest_version":"0.1.1",
          "linux_package_root":"songcore-v0.1.1-linux-x86_64",
          "windows_package_root":"songcore-v0.1.1-windows-x86_64",
          "component_version_mode":"independent","component_version":"0.1.1",
          "xmake_project_version":"0.1.0"}
        path=root/"decision.json"
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
        mismatch=dict(decision,windows_package_root="songcore-v9.9.9-windows-x86_64")
        expect_red("VERSION-MAPPING",version_run(mismatch),"windows_package_root")
        xmake_lie=dict(decision,xmake_project_version="9.9.9")
        expect_red("VERSION-XMAKE",version_run(xmake_lie),"native/xmake.lua")
        coupled=dict(decision,component_version_mode="project-coupled")
        expect_red("VERSION-COUPLING",version_run(coupled),"project-coupled")
    workflow=ROOT/".github/workflows/songcore-release-substrate.yml"
    if not workflow.is_file(): raise AssertionError("ops release substrate workflow missing")
    text=workflow.read_text(encoding="utf-8")
    for trigger in ("ops/**","native/ffmpeg/targets/**","native/ffmpeg/profiles/**","native/ffmpeg/pin.json",
                    "LICENSE-MIT","LICENSE-APACHE","docs/release/songcore-release-contract.md",
                    "docs/release/songcore-version-decision.json",
                    "evidence/songcore-172/linux/verify/package/external-consumer-example.c",
                    "tools/verify_songcore_build_receipt.py","tools/verify_songcore_version_identity.py",
                    "tools/test_songcore_release_substrate.py","tools/docs-verify.cmd"):
        if trigger not in text: raise AssertionError(f"workflow trigger missing: {trigger}")
    linux=(ROOT/"ops/package-linux.sh").read_text(encoding="utf-8")
    windows=(ROOT/"ops/package-rc.ps1").read_text(encoding="utf-8")
    build_linux=(ROOT/"ops/build-linux.sh").read_text(encoding="utf-8")
    build_windows=(ROOT/"ops/build-rc.ps1").read_text(encoding="utf-8")
    ffmpeg=(ROOT/"native/build/ffmpeg.lua").read_text(encoding="utf-8")
    version_verifier=VERIFY_VERSION.read_text(encoding="utf-8")
    required={
      "linux receipt required":"--build-receipt is required for release-qualified packaging",
      "linux strict version":"suffixes are not authorized",
      "windows receipt required":"[Parameter(Mandatory=$true)][string]$BuildReceipt",
      "windows strict version":"suffixes are not authorized by contract section 2",
      "linux cache profile":"man.get(\"profile_sha256\") == profile_sha",
      "linux compile evidence":"int(d[\"compile_actions\"])",
      "linux receipt header identity":"include/songcore.h src/songcore_ffmpeg.c",
      "xmake recipe binding":"mt.recipe_sha256 ~= recipe_sha256",
      "linux shared verifier":"verify_songcore_build_receipt.py\" \\",
      "windows shared verifier":"verify_songcore_build_receipt.py\")",
      "linux owner version gate":"Q1 version identity is OWNER_BLOCKED",
      "windows owner version gate":"Q1 version identity is OWNER_BLOCKED",
    }
    haystacks={"linux receipt required":linux,"linux strict version":linux,"windows receipt required":windows,
      "windows strict version":windows,"linux cache profile":build_linux,"linux compile evidence":build_linux,
      "linux receipt header identity":build_linux,
      "xmake recipe binding":ffmpeg,"linux shared verifier":linux,"windows shared verifier":windows,
      "linux owner version gate":version_verifier,"windows owner version gate":version_verifier}
    for name,needle in required.items():
        if needle not in haystacks[name]: raise AssertionError(f"production guard missing: {name}")
    clean = 'xmake clean -a > "$EV/clean.log" 2>&1'
    configure = 'xmake f -p mingw -a x86_64 -m release -y > "$EV/config.log" 2>&1'
    if build_windows.index(clean) > build_windows.index(configure):
        raise AssertionError("Windows build config is erased by a later xmake clean -a")
    decision=json.loads((ROOT/"docs/release/songcore-version-decision.json").read_text(encoding="utf-8"))
    if decision.get("status") not in ("OWNER_BLOCKED","APPROVED"):
        raise AssertionError("unknown Q1 decision state")
    print("PASS: SongCore release substrate negative controls M1-M10"); return 0

if __name__ == "__main__": raise SystemExit(main())

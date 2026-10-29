#!/usr/bin/env python3
"""Replicate test_release.py's native-synthetic-baseline fixture and surface
check_release.py's real stderr (its exit-3 exception text is swallowed by the
harness's expected-message comparison). Read-only diagnostic; writes nothing
into the repo."""
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools"))
import check_release as cr  # noqa: E402

ROOT = cr.ROOT
print("cr.FILES:", cr.FILES)

with tempfile.TemporaryDirectory(prefix="relfix-") as tmp:
    tmp = Path(tmp)
    repo = tmp / "repo"; repo.mkdir()
    for f in ("Cargo.toml", "CHANGELOG.md", *cr.FILES[1:]):
        shutil.copyfile(ROOT / f, repo / f)
    shutil.copytree(ROOT / "native/ffmpeg", repo / "native/ffmpeg")
    (repo / "native/include").mkdir()
    shutil.copyfile(ROOT / "native/include/songcore.h", repo / "native/include/songcore.h")
    def git(*a):
        subprocess.run(["git", *a], cwd=repo, check=True, capture_output=True)
    git("init", "-q"); git("add", ".")
    git("-c", "user.name=Diag", "-c", "user.email=d@x.invalid", "commit", "-qm", "fixture")
    sha = subprocess.run(["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True, text=True).stdout.strip()
    v = cr.tomllib.loads((repo / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    tag = "qianqian-v" + v

    pe = tmp / "pe"; pe.mkdir()
    (pe / "main.c").write_text("int main(void) { return 0; }\n")
    subprocess.run(["x86_64-w64-mingw32-gcc", "main.c", "-o", "good.exe"], cwd=pe, check=True, capture_output=True)
    native = tmp / "native"; (native / "build/artifacts").mkdir(parents=True)
    (native / "build/ffmpeg-xmake").mkdir()
    shutil.copytree(repo / "native/include", native / "include")
    subprocess.run(["x86_64-w64-mingw32-gcc", "-c", "main.c", "-o", "sample.o"], cwd=pe, check=True, capture_output=True)
    subprocess.run(["x86_64-w64-mingw32-ar", "rcs", str(native / "build/artifacts/libsongcore.a"), "sample.o"], cwd=pe, check=True, capture_output=True)
    pin = cr.read_json(repo / "native/ffmpeg/pin.json")
    closure = {"schema": 2, "ffmpeg_tag": pin["ffmpeg_tag"], "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
               "ffmpeg_source_sha256": pin["source_sha256"],
               "profile_sha256": cr.file_hash(repo / "native/ffmpeg/profiles/codec-base.json"),
               "target": {"id": "windows-x86_64", "platform": "windows", "arch": "x86_64",
                          "recipe_sha256": cr.file_hash(repo / "native/ffmpeg/targets/windows-x86_64.json"),
                          "target_os": "mingw32", "cross_prefix": "x86_64-w64-mingw32-"},
               "units": [{"path": "synthetic-contract-control.c"}]}
    cr.write_json(native / "build/ffmpeg-xmake/manifest.json", closure)

    cmd = [sys.executable, str(ROOT / "tools/check_release.py"), "native",
           "--repo", str(repo), "--tag", tag, "--source", sha, "--mode", "dry-run",
           "--native", str(native)]
    print("CMD:", cmd)
    p = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    print("EXIT:", p.returncode)
    print("--- stdout ---"); print(p.stdout)
    print("--- stderr ---"); print(p.stderr)

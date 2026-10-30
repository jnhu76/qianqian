#!/usr/bin/env bash
set -u
echo "argv: git ls-remote https://github.com/FFmpeg/FFmpeg refs/tags/n9.0.1^{}"
git ls-remote https://github.com/FFmpeg/FFmpeg 'refs/tags/n9.0.1^{}'
rc=$?
echo "git_ls_remote_exit=$rc"
exit "$rc"

set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
for t in xmake python3 x86_64-w64-mingw32-gcc x86_64-w64-mingw32-objdump x86_64-w64-mingw32-nm tar unzip sha256sum; do
    command -v "$t" > /dev/null || { echo "missing tool: $t"; exit 10; }
    echo "tool ok: $t -> $(command -v "$t")"
done
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
for t in xmake python3 x86_64-w64-mingw32-gcc x86_64-w64-mingw32-ar \
         x86_64-w64-mingw32-ld x86_64-w64-mingw32-objdump x86_64-w64-mingw32-nm \
         tar sha256sum; do
    command -v "$t" > /dev/null || { echo "missing tool: $t"; exit 10; }
    echo "tool ok: $t -> $(command -v "$t")"
done
echo "TOOL_XMAKE=$(xmake --version | head -1)"
echo "TOOL_CC=$(x86_64-w64-mingw32-gcc --version | head -1)"
echo "TOOL_AR=$(x86_64-w64-mingw32-ar --version | head -1)"
echo "TOOL_LD=$(x86_64-w64-mingw32-ld --version | head -1)"
echo "TOOL_PYTHON=$(python3 --version 2>&1)"
echo "TOOLS_OK"
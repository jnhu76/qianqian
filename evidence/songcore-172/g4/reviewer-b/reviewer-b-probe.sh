set -u
WI=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64
LI=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64
echo "### WINDOWS STATIC ARCHIVE (pe-x86-64) defined T symbols in the 15 ABI set"
x86_64-w64-mingw32-nm --defined-only -g "$WI/lib/libsongcore.a" 2>/dev/null | awk '{print $NF}' | sort -u > /tmp/win_def.txt
for s in $(cat "$WI/metadata/symbols.txt"); do
  if grep -qx "$s" /tmp/win_def.txt; then echo "PRESENT $s"; else echo "MISSING $s"; fi
done
echo "count_present=$(grep -c . /tmp/win_def.txt)"
echo
echo "### LINUX STATIC ARCHIVE (elf64-x86-64) defined T symbols"
nm --defined-only -g "$LI/lib/libsongcore.a" 2>/dev/null | awk '{print $NF}' | sort -u > /tmp/lin_def.txt
for s in $(cat "$LI/metadata/symbols.txt"); do
  if grep -qx "$s" /tmp/lin_def.txt; then echo "PRESENT $s"; else echo "MISSING $s"; fi
done
echo
echo "### LINUX SHARED dynsym exported (global default-vis) defined"
readelf --dyn-syms -W "$LI/lib/shared/libsongcore.so" | awk '$7!="UND" && $5=="GLOBAL" {print $8}' | sed 's/@.*//' | sort -u > /tmp/so_exp.txt
cat /tmp/so_exp.txt
echo "so_export_count=$(grep -c . /tmp/so_exp.txt)"
echo
echo "### set difference: header 15 vs .so exports"
grep -oP '^SONGCORE_API [^ ]+ \K(song[A-Za-z_]+)' /mnt/c/Users/fred1/source/qianqian/native/include/songcore.h | sort -u > /tmp/hdr.txt
echo "header_count=$(grep -c . /tmp/hdr.txt)"
echo "only_in_header:"; comm -23 /tmp/hdr.txt /tmp/so_exp.txt
echo "only_in_so:"; comm -13 /tmp/hdr.txt /tmp/so_exp.txt
echo
echo "### SONAME / NEEDED"
readelf -d -W "$LI/lib/shared/libsongcore.so" | grep -E 'SONAME|RPATH|RUNPATH|NEEDED'

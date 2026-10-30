#!/usr/bin/env bash
# Lane E: ELF runtime-dependency / glibc symbol-version measurement.
# Read-only over the build artifacts; writes only to the lane evidence dir.
set -u
export LC_ALL=C
set -o pipefail

ART=/home/hoo/g4-172/linux-root/src/native/build/artifacts
SO="$ART/shared/libsongcore.so"
AR="$ART/libsongcore.a"
EVD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$EVD/raw"
mkdir -p "$RAW"
LOG="$RAW/exitcodes.txt"
: > "$LOG"
note() { printf '%s | argv: %s | exit=%s | output=%s\n' "$1" "$2" "$3" "$4" >> "$LOG"; }

# ---------------- artifact identity ----------------
file "$SO" "$AR" > "$RAW/file-artifacts.txt" 2>&1
note "file artifacts" "file $SO $AR" "$?" "$RAW/file-artifacts.txt"

sha256sum "$SO" "$AR" > "$RAW/sha256-artifacts.txt" 2>&1
note "sha256 artifacts" "sha256sum $SO $AR" "$?" "$RAW/sha256-artifacts.txt"

ar t "$AR" > "$RAW/ar-members.txt" 2>&1
note "ar member list" "ar t $AR" "$?" "$RAW/ar-members.txt"
wc -l < "$RAW/ar-members.txt" > "$RAW/ar-member-count.txt"

# ---------------- shared object: dynamic symbol/version data ----------------
objdump -T "$SO" > "$RAW/objdump-T-so.txt" 2>&1
note "objdump -T so" "objdump -T $SO" "$?" "$RAW/objdump-T-so.txt"

objdump -T "$SO" | grep -o 'GLIBC_[0-9.]*' | sort -uV > "$RAW/glibc-versions.txt" 2>&1
note "glibc versions (objdump -T)" "objdump -T $SO | grep -o 'GLIBC_[0-9.]*' | sort -uV" "$?" "$RAW/glibc-versions.txt"

tail -n 1 "$RAW/glibc-versions.txt" > "$RAW/glibc-highest.txt" 2>&1
note "glibc highest" "tail -n 1 $RAW/glibc-versions.txt" "$?" "$RAW/glibc-highest.txt"

readelf --version-info "$SO" > "$RAW/readelf-version-info-so.txt" 2>&1
note "readelf --version-info so" "readelf --version-info $SO" "$?" "$RAW/readelf-version-info-so.txt"

readelf --dyn-syms --wide "$SO" > "$RAW/readelf-dyn-syms-so.txt" 2>&1
note "readelf --dyn-syms so" "readelf --dyn-syms --wide $SO" "$?" "$RAW/readelf-dyn-syms-so.txt"

# every version tag attached to a dynamic symbol (GLIBC_*, and any other namespace)
awk '{print $8}' "$RAW/readelf-dyn-syms-so.txt" | grep '@' | sed 's/.*@//' | sort -uV > "$RAW/dynsym-version-tags.txt" 2>&1
note "dynsym version tags" "readelf --dyn-syms --wide \$SO | awk '{print \$8}' | grep @ | sed s/.*@// | sort -uV" "$?" "$RAW/dynsym-version-tags.txt"

# non-GLIBC version namespaces explicitly (GLIBCXX / CXXABI / GCC / GOMP / ...)
grep -E '^(GLIBCXX|CXXABI|GCC|GOMP|LIBATOMIC|GLIBC)_' "$RAW/dynsym-version-tags.txt" > "$RAW/non-glibc-version-tags.txt" 2>&1
note "non-GLIBC version namespaces in dynsym tags" "grep -E '^(GLIBCXX|CXXABI|GCC|GOMP|LIBATOMIC|GLIBC)_' $RAW/dynsym-version-tags.txt" "$?" "$RAW/non-glibc-version-tags.txt"

# ---------------- shared object: ELF headers / dynamic section ----------------
readelf -h "$SO" > "$RAW/readelf-h-so.txt" 2>&1
note "readelf -h so" "readelf -h $SO" "$?" "$RAW/readelf-h-so.txt"

readelf -l "$SO" > "$RAW/readelf-l-so.txt" 2>&1
note "readelf -l so" "readelf -l $SO" "$?" "$RAW/readelf-l-so.txt"

readelf -d "$SO" > "$RAW/readelf-dynamic-so.txt" 2>&1
note "readelf -d so" "readelf -d $SO" "$?" "$RAW/readelf-dynamic-so.txt"

objdump -p "$SO" > "$RAW/objdump-p-so.txt" 2>&1
note "objdump -p so" "objdump -p $SO" "$?" "$RAW/objdump-p-so.txt"

nm -D --defined-only "$SO" > "$RAW/nm-D-defined-so.txt" 2>&1
note "nm -D --defined-only so" "nm -D --defined-only $SO" "$?" "$RAW/nm-D-defined-so.txt"

nm -D --undefined-only "$SO" > "$RAW/nm-D-undefined-so.txt" 2>&1
note "nm -D --undefined-only so" "nm -D --undefined-only $SO" "$?" "$RAW/nm-D-undefined-so.txt"

nm -D --defined-only --format=posix "$SO" | awk '$2 ~ /^[TtWwSs]$/ {print $1}' | grep -E '^song' | sort > "$RAW/exported-song-symbols.txt" 2>&1
note "exported song* symbols" "nm -D --defined-only --format=posix \$SO | awk ... | grep -E '^song' | sort" "$?" "$RAW/exported-song-symbols.txt"
wc -l < "$RAW/exported-song-symbols.txt" > "$RAW/exported-song-symbol-count.txt"

# NEEDED (machine-readable)
grep '(NEEDED)' "$RAW/readelf-dynamic-so.txt" > "$RAW/needed-libs.txt" 2>&1
note "NEEDED lines" "grep '(NEEDED)' $RAW/readelf-dynamic-so.txt" "$?" "$RAW/needed-libs.txt"
grep -E '(SONAME|RPATH|RUNPATH)' "$RAW/readelf-dynamic-so.txt" > "$RAW/soname-rpath.txt" 2>&1
note "SONAME/RPATH" "grep -E '(SONAME|RPATH|RUNPATH)' $RAW/readelf-dynamic-so.txt" "$?" "$RAW/soname-rpath.txt"

# ---------------- ldd in an isolated directory ----------------
ISO="$HOME/lane-e-runtime/isolate"
rm -rf "$ISO"; mkdir -p "$ISO"
cp "$SO" "$ISO/"
( cd "$ISO" && ldd ./libsongcore.so ) > "$RAW/ldd-isolated-so.txt" 2>&1
note "ldd isolated" "(cd $ISO && ldd ./libsongcore.so)" "$?" "$RAW/ldd-isolated-so.txt"

( cd "$ISO" && readelf -d ./libsongcore.so | grep -E '(NEEDED|SONAME|RPATH|RUNPATH)' ) > "$RAW/readelf-d-isolated-so.txt" 2>&1
note "readelf -d isolated" "(cd $ISO && readelf -d ./libsongcore.so | grep -E ...)" "$?" "$RAW/readelf-d-isolated-so.txt"

# ---------------- summary ----------------
{
  echo "artifact_sha256:"
  cat "$RAW/sha256-artifacts.txt"
  echo
  echo "ar_members: $(cat "$RAW/ar-member-count.txt")"
  echo "glibc_versions_required:"
  cat "$RAW/glibc-versions.txt"
  echo "glibc_highest_required: $(cat "$RAW/glibc-highest.txt")"
  echo
  echo "needed_libs:"
  cat "$RAW/needed-libs.txt"
  echo
  echo "dynamic_sym_version_tags:"
  cat "$RAW/dynsym-version-tags.txt"
  echo
  echo "exported_song_symbols: $(cat "$RAW/exported-song-symbol-count.txt")"
  cat "$RAW/exported-song-symbols.txt"
  echo
  echo "ldd_isolated:"
  cat "$RAW/ldd-isolated-so.txt"
} > "$EVD/summary-elf.txt" 2>&1
note "summary" "internal summary assembly" "$?" "$EVD/summary-elf.txt"

echo "done"

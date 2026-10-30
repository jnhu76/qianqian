set -u
WI=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64
W=/tmp/rb-win; rm -rf $W; mkdir -p $W
cat > $W/all15.c <<'C'
#include <stdio.h>
#include "songcore.h"
int main(void) {
    void *p[15];
    p[0]=(void*)songcore_abi_version; p[1]=(void*)song_open; p[2]=(void*)song_probe;
    p[3]=(void*)song_audio_stream_count; p[4]=(void*)song_audio_stream_info; p[5]=(void*)song_select_stream;
    p[6]=(void*)song_get_metadata; p[7]=(void*)song_get_metadata_count; p[8]=(void*)song_get_metadata_entry;
    p[9]=(void*)song_get_artwork_count; p[10]=(void*)song_get_artwork_item; p[11]=(void*)song_last_error;
    p[12]=(void*)song_read_pcm; p[13]=(void*)song_seek; p[14]=(void*)song_close;
    for (int i=0;i<15;i++) if(!p[i]) { printf("NULL %d\n",i); return 2; }
    printf("all15_addresses_ok abi=%u host=windows_pe\n", (unsigned)songcore_abi_version());
    fflush(stdout);
    return 0;
}
C
echo "### mingw link WITHOUT -lbcrypt (negative control)"
x86_64-w64-mingw32-gcc -O2 -I $WI/include $W/all15.c -L $WI/lib -lsongcore -o $W/all15-nobcrypt.exe 2>&1 | tail -5
echo "link_exit(no bcrypt)=${PIPESTATUS[0]}"
echo
echo "### mingw link WITH -lbcrypt"
x86_64-w64-mingw32-gcc -O2 -I $WI/include $W/all15.c -L $WI/lib -lsongcore -lbcrypt -o $W/all15.exe 2>&1 | tail -5
echo "link_exit=$?"
cp -f $W/all15.exe /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/reviewer-b-all15.exe
echo "copied_to_windows=yes"

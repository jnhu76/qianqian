set -u
LI=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64
W=/tmp/rb-link; rm -rf $W; mkdir -p $W
cat > $W/all15.c <<'C'
#include <stdio.h>
#include "songcore.h"
int main(void) {
    void *p[15];
    p[0]=(void*)songcore_abi_version;
    p[1]=(void*)song_open;
    p[2]=(void*)song_probe;
    p[3]=(void*)song_audio_stream_count;
    p[4]=(void*)song_audio_stream_info;
    p[5]=(void*)song_select_stream;
    p[6]=(void*)song_get_metadata;
    p[7]=(void*)song_get_metadata_count;
    p[8]=(void*)song_get_metadata_entry;
    p[9]=(void*)song_get_artwork_count;
    p[10]=(void*)song_get_artwork_item;
    p[11]=(void*)song_last_error;
    p[12]=(void*)song_read_pcm;
    p[13]=(void*)song_seek;
    p[14]=(void*)song_close;
    for (int i=0;i<15;i++) if(!p[i]) { printf("NULL %d\n",i); return 2; }
    printf("all15_addresses_ok abi=%u\n", (unsigned)songcore_abi_version());
    return 0;
}
C
echo "### static link against packaged lib/libsongcore.a"
gcc -O2 -I $LI/include $W/all15.c $LI/lib/libsongcore.a -lm -lpthread -o $W/all15-static; echo "link_exit=$?"
$W/all15-static; echo "run_exit=$?"
echo
echo "### shared link against packaged lib/shared"
gcc -O2 -I $LI/include $W/all15.c -L $LI/lib/shared -lsongcore -lm -lpthread -Wl,-rpath,$LI/lib/shared -o $W/all15-shared; echo "link_exit=$?"
$W/all15-shared; echo "run_exit=$?"
echo
echo "### undefined-after-link check (nm -u on the static-linked binary for song*)"
nm -u $W/all15-static | grep ' song' || echo "no undefined song* symbols"

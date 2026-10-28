use std::ffi::c_void;
type Handle = *mut c_void;
#[link(name="kernel32")]
unsafe extern "system" {
    fn CreatePipe(r:*mut Handle,w:*mut Handle,sa:*mut c_void,size:u32)->i32;
    fn GetNamedPipeInfo(h:Handle,flags:*mut u32,outsz:*mut u32,insz:*mut u32,maxinst:*mut u32)->i32;
    fn WriteFile(h:Handle,buf:*const u8,len:u32,written:*mut u32,ov:*mut c_void)->i32;
    fn PeekNamedPipe(h:Handle,buf:*mut u8,blen:u32,bread:*mut u32,avail:*mut u32,left:*mut u32)->i32;
    fn GetLastError()->u32;
}
fn main(){
    for requested in [0u32, 4096, 65536] {
        let (mut r, mut w): (Handle, Handle) = (std::ptr::null_mut(), std::ptr::null_mut());
        let ok = unsafe { CreatePipe(&mut r,&mut w,std::ptr::null_mut(),requested) };
        let (mut flags, mut outsz, mut insz, mut maxinst) = (0u32,0u32,0u32,0u32);
        let qi = unsafe { GetNamedPipeInfo(r,&mut flags,&mut outsz,&mut insz,&mut maxinst) };
        let ge = unsafe { GetLastError() };
        // fill: write 4096 repeatedly until blocked is hard; just write 4096 and peek
        let buf = vec![b'F'; 4096]; let mut n=0u32;
        let wok = unsafe { WriteFile(w, buf.as_ptr(), 4096, &mut n, std::ptr::null_mut()) };
        let (mut avail, mut left) = (0u32,0u32);
        let pk = unsafe { PeekNamedPipe(r,std::ptr::null_mut(),0,std::ptr::null_mut(),&mut avail,&mut left) };
        println!("req={requested} create={ok} info={qi} err={ge} flags={flags} outsz={outsz} insz={insz} maxinst={maxinst} write4096={wok}/{n} peek={pk} avail={avail}");
    }
}

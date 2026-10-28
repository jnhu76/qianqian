use std::ffi::c_void;
use std::sync::mpsc;
use std::time::Duration;
type Handle = *mut c_void;
#[link(name="kernel32")]
unsafe extern "system" {
    fn CreatePipe(r:*mut Handle,w:*mut Handle,sa:*mut c_void,size:u32)->i32;
    fn GetNamedPipeInfo(h:Handle,flags:*mut u32,outsz:*mut u32,insz:*mut u32,maxinst:*mut u32)->i32;
    fn WriteFile(h:Handle,buf:*const u8,len:u32,written:*mut u32,ov:*mut c_void)->i32;
    fn PeekNamedPipe(h:Handle,buf:*mut u8,blen:u32,bread:*mut u32,avail:*mut u32,left:*mut u32)->i32;
}
fn main(){
    for nbytes in [4096usize, 65536, 1<<20, 16<<20] {
        let (mut r, mut w): (Handle, Handle) = (std::ptr::null_mut(), std::ptr::null_mut());
        unsafe { CreatePipe(&mut r,&mut w,std::ptr::null_mut(),0) };
        let (mut f,mut o,mut i,mut m)=(0u32,0u32,0u32,0u32);
        unsafe { GetNamedPipeInfo(r,&mut f,&mut o,&mut i,&mut m) };
        let (tx,rx)=mpsc::channel();
        let h=w as usize;
        std::thread::spawn(move||{
            let buf=vec![b'F'; nbytes];
            let mut n=0u32;
            let ok=unsafe{WriteFile(h as Handle, buf.as_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut())};
            let _=tx.send((ok,n));
        });
        std::thread::sleep(Duration::from_millis(300));
        let (mut avail,mut left)=(0u32,0u32);
        unsafe{PeekNamedPipe(r,std::ptr::null_mut(),0,std::ptr::null_mut(),&mut avail,&mut left)};
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(v)=>println!("bytes={nbytes} capacity_hint={i} write_completed={v:?} avail={avail}"),
            Err(_)=>println!("bytes={nbytes} capacity_hint={i} write_completed=NO(still-blocked) avail={avail}"),
        }
    }
}

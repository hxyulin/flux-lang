use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};

unsafe extern "C" {
    fn pipe(fds: *mut i32) -> i32;
    fn dup(fd: i32) -> i32;
    fn dup2(from: i32, to: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn fcntl(fd: i32, operation: i32, ...) -> i32;
    fn openpty(
        master: *mut i32,
        slave: *mut i32,
        name: *mut u8,
        termios: *const core::ffi::c_void,
        winsize: *const core::ffi::c_void,
    ) -> i32;
    fn signal(number: i32, handler: usize) -> usize;
}
struct Redirect {
    fd: i32,
    saved: i32,
    read: std::fs::File,
    write: i32,
}
impl Redirect {
    fn new(fd: i32) -> Redirect {
        if fd == 1 {
            io::stdout().flush().unwrap();
        }
        if fd == 2 {
            io::stderr().flush().unwrap();
        }
        let mut fds = [0; 2];
        // SAFETY: pipe initializes two fresh descriptors, and dup/dup2 retain
        // the original standard descriptor until finish restores it.
        unsafe {
            assert_eq!(pipe(fds.as_mut_ptr()), 0);
            let saved = dup(fd);
            assert!(saved >= 0);
            assert_eq!(dup2(fds[1], fd), fd);
            Redirect {
                fd,
                saved,
                read: std::fs::File::from_raw_fd(fds[0]),
                write: fds[1],
            }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.fd == 1 {
            io::stdout().flush().unwrap();
        }
        if self.fd == 2 {
            io::stderr().flush().unwrap();
        }
        // SAFETY: restore the retained original before closing our pipe writer.
        // Closing the last writer makes read_to_end finish at actual EOF.
        unsafe {
            assert_eq!(dup2(self.saved, self.fd), self.fd);
            close(self.saved);
            close(self.write);
        }
        let mut bytes = Vec::new();
        self.read.read_to_end(&mut bytes).unwrap();
        bytes
    }
}
struct Reenter;
impl std::fmt::Display for Reenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        print!("nested");
        f.write_str("fmt")
    }
}
fn main() {
    let mut tty_master = 0;
    let mut tty_slave = 0;
    // SAFETY: fresh owned PTY endpoints initialize valid descriptor slots.
    unsafe {
        assert_eq!(
            openpty(
                &mut tty_master,
                &mut tty_slave,
                core::ptr::null_mut(),
                core::ptr::null(),
                core::ptr::null()
            ),
            0
        );
        let master = std::fs::File::from_raw_fd(tty_master);
        let slave = std::fs::File::from_raw_fd(tty_slave);
        assert!(master.is_terminal());
        assert!(slave.is_terminal());
    }
    assert_eq!(io::stdout().as_raw_fd(), 1);
    assert_eq!(io::stderr().as_raw_fd(), 2);
    let capture = Redirect::new(1);
    assert!(!io::stdout().is_terminal());
    assert!(!capture.read.is_terminal());
    print!("left");
    io::stdout().write_all(b"middle").unwrap();
    {
        let mut locked = io::stdout().lock();
        assert!(!locked.is_terminal());
        assert_eq!(locked.as_raw_fd(), 1);
        print!("inside");
        locked.write_all(b"]").unwrap();
        write!(locked, "{}", Reenter).unwrap();
        locked.flush().unwrap();
    }
    // Closed standard streams act as sinks; they do not retain discarded data
    // that could appear later when the standard descriptor is restored.
    unsafe {
        assert_eq!(close(1), 0);
    }
    io::stdout().write_all(b"discard").unwrap();
    io::stdout().flush().unwrap();
    unsafe {
        assert_eq!(dup2(capture.write, 1), 1);
    }
    io::stdout().write_all(b"tail").unwrap();
    io::stdout().flush().unwrap();
    assert_eq!(capture.finish(), b"leftmiddleinside]nestedfmttail");

    let capture = Redirect::new(2);
    eprint!("a");
    io::stderr().write_all(b"b").unwrap();
    {
        let mut locked = io::stderr().lock();
        assert!(!locked.is_terminal());
        eprint!("c");
        write!(locked, "d").unwrap();
    }
    assert_eq!(capture.finish(), b"abcd");

    let capture = Redirect::new(1);
    let saved_error = unsafe { dup(2) };
    unsafe {
        assert_eq!(dup2(1, 2), 2);
    }
    print!("a");
    eprint!("b");
    io::stdout().write_all(b"c").unwrap();
    io::stderr().write_all(b"d").unwrap();
    io::stdout().flush().unwrap();
    unsafe {
        assert_eq!(dup2(saved_error, 2), 2);
        close(saved_error);
    }
    assert_eq!(capture.finish(), b"bdac");

    let mut fds = [0; 2];
    unsafe {
        assert_eq!(pipe(fds.as_mut_ptr()), 0);
    }
    let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
    writer.write_all("é\nrest".as_bytes()).unwrap();
    drop(writer);
    let old = unsafe { dup(0) };
    unsafe {
        assert_eq!(dup2(fds[0], 0), 0);
        close(fds[0]);
    }
    assert!(!io::stdin().is_terminal());
    let mut line = String::new();
    assert_eq!(io::stdin().read_line(&mut line).unwrap(), 3);
    assert_eq!(line, "é\n");
    {
        let mut input = io::stdin().lock();
        assert!(!input.is_terminal());
        assert_eq!(input.as_raw_fd(), 0);
        assert_eq!(input.fill_buf().unwrap(), b"rest");
        input.consume(1);
    }
    let mut rest = Vec::new();
    (&io::stdin()).take(2).read_to_end(&mut rest).unwrap();
    assert_eq!(rest, b"es");
    let mut tail = String::new();
    io::stdin().read_to_string(&mut tail).unwrap();
    assert_eq!(tail, "t");
    unsafe {
        assert_eq!(dup2(old, 0), 0);
        close(old);
    }

    let capture = Redirect::new(1);
    std::thread::scope(|scope| {
        for id in 0..4 {
            scope.spawn(move || {
                for _ in 0..20 {
                    if id == 3 {
                        // This writer never takes a stdio facade guard: the
                        // print runtime itself must join the same global lock.
                        println!("<{}:x>", id);
                    } else {
                        let mut locked = io::stdout().lock();
                        write!(locked, "<{}:", id).unwrap();
                        print!("x");
                        writeln!(locked, ">").unwrap();
                    }
                }
            });
        }
    });
    let output = capture.finish();
    assert_eq!(output.len(), 480);
    let mut counts = [0; 4];
    for row in output.chunks_exact(6) {
        assert_eq!(row[0], b'<');
        assert!(row[1] >= b'0' && row[1] <= b'3');
        assert_eq!(&row[2..], b":x>\n");
        counts[(row[1] - b'0') as usize] += 1;
    }
    assert_eq!(counts, [20; 4]);

    let mut capture = Redirect::new(1);
    unsafe {
        let flags = fcntl(1, 3);
        assert!(flags >= 0);
        assert_eq!(fcntl(1, 4, flags | 4), 0);
        let flags = fcntl(capture.read.as_raw_fd(), 3);
        assert!(flags >= 0);
        assert_eq!(fcntl(capture.read.as_raw_fd(), 4, flags | 4), 0);
    }
    let large = vec![b'P'; 131072];
    let first = io::stdout().write(&large).unwrap();
    assert!(first > 0 && first < large.len());
    loop {
        match io::stdout().write(&large) {
            Ok(count) => assert!(count > 0),
            Err(error) => {
                assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
                break;
            }
        }
    }
    io::stdout().write_all(b"pending").unwrap();
    assert_eq!(
        io::stdout().flush().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let mut block = [0; 4096];
    loop {
        match capture.read.read(&mut block) {
            Ok(0) => panic!("pipe unexpectedly ended"),
            Ok(count) => assert!(block[..count].iter().all(|&byte| byte == b'P')),
            Err(error) => {
                assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
                break;
            }
        }
    }
    io::stdout().flush().unwrap();
    assert_eq!(capture.finish(), b"pending");

    // Isolate write error propagation from the process's configurable SIGPIPE
    // disposition; the standard signal API represents SIG_IGN as handler tag 1.
    unsafe {
        signal(13, 1);
    }
    let mut closed = [0; 2];
    unsafe {
        assert_eq!(pipe(closed.as_mut_ptr()), 0);
        close(closed[0]);
    }
    let mut broken = unsafe { std::fs::File::from_raw_fd(closed[1]) };
    assert_eq!(
        broken.write(b"broken").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    println!("stdio ordered terminal buffered limits concurrent errors");
}

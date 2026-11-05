// Environment, PATH-list and path-manipulation APIs that registry crates
// and build scripts use.
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf, MAIN_SEPARATOR, MAIN_SEPARATOR_STR};

fn main() {
    let vars: Vec<(String, String)> = env::vars().filter(|(name, _)| name == "HOME" || name == "PATH").collect();
    println!("vars {} {}", vars.len(), env::vars().count() == env::vars_os().count());
    println!("args {} {}", env::args().len(), env::args().skip(1).count());
    let home = env::home_dir();
    println!("home {}", home.map(|path| path.is_absolute()).unwrap_or(false));

    let split: Vec<PathBuf> = env::split_paths("/usr/bin::/bin:").collect();
    println!("{:?}", split);
    println!("{:?}", env::split_paths("").collect::<Vec<_>>());
    let joined = env::join_paths(["/a", "b/c", ""]).unwrap();
    println!("{:?}", joined);
    match env::join_paths(["x:y"]) {
        Ok(_) => println!("joined"),
        Err(error) => println!("{error} {error:?}"),
    }

    println!("{MAIN_SEPARATOR} {MAIN_SEPARATOR_STR} {}", std::path::is_separator('/'));
    let mut path = PathBuf::from("/usr/local/lib/");
    println!("{} {:?}", path.pop(), path);
    println!("{} {:?}", path.pop(), path);
    println!("{} {:?}", path.pop(), path);
    println!("{} {:?}", path.pop(), path);
    let mut relative = PathBuf::from("a");
    let first = relative.pop();
    println!("{} {:?}", first, relative);
    println!("{}", relative.pop());
    let mut named = PathBuf::from("dir/file.txt");
    named.set_file_name("other.rs");
    println!("{:?}", named);
    let mut bare = PathBuf::from("/");
    bare.set_file_name("x");
    println!("{:?}", bare);
    let collected: PathBuf = ["a", "b", "/c", "d"].iter().collect();
    println!("{:?}", collected);
    let parts: Vec<&OsStr> = Path::new("/a/./b//c/").iter().collect();
    println!("{:?}", parts);
    for part in Path::new("x/y") {
        print!("{:?} ", part);
    }
    println!();
    println!("{} {} {}", Path::new("/a/b/c").ends_with("b/c"), Path::new("/a/b/c").ends_with("a/b"), Path::new("a").ends_with(""));
    println!("{:?} {:?} {:?}", Path::new("x/archive.tar.gz").file_prefix(), Path::new(".hidden.rc").file_prefix(), Path::new("/").file_prefix());
    let boxed: Box<Path> = Box::from(Path::new("boxed/path"));
    println!("{:?} {:?}", boxed, PathBuf::from(boxed.clone()));
    let os: OsString = PathBuf::from("into/os").into();
    println!("{:?}", os);

    let mut text = OsString::with_capacity(4);
    text.push("ab");
    text.push(OsStr::new("cd"));
    text.push(String::from("ef"));
    println!("{:?} {} {}", text, text.capacity() >= 6, text.len());
    println!("{:?} {}", text.to_ascii_uppercase(), OsStr::new("ABC").eq_ignore_ascii_case("abc"));
    println!("{}", OsStr::new("shown").display());
    let pieces = [OsStr::new("a"), OsStr::new("b"), OsStr::new("c")];
    println!("{:?}", pieces.join(OsStr::new("/")));
    text.clear();
    println!("{:?} {}", text, text.is_empty());
}

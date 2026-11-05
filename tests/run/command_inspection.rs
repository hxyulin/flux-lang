// Command's program, arguments, environment changes and directory.
use std::process::Command;

fn main() {
    let mut command = Command::new("tool");
    println!("{:?} {} {}", command.get_program(), command.get_args().len(), command.get_envs().len());
    command
        .args(["--flag", "value with spaces", ""])
        .env("ZETA", "last")
        .env("ALPHA", "first")
        .env_remove("MIDDLE")
        .env("ALPHA", "replaced")
        .current_dir("some/dir");
    let args: Vec<_> = command.get_args().collect();
    println!("{args:?}");
    let mut iter = command.get_args();
    println!("{} {:?}", iter.len(), iter.next());
    println!("{} {:?}", iter.len(), iter.size_hint());
    let envs: Vec<_> = command.get_envs().collect();
    println!("{envs:?}");
    for (name, value) in command.get_envs() {
        match value {
            Some(value) => println!("{} = {}", name.to_string_lossy(), value.to_string_lossy()),
            None => println!("{} removed", name.to_string_lossy()),
        }
    }
    println!("{:?} {:?}", command.get_program(), command.get_current_dir());
}

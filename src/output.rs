use std::fmt::Arguments;
use std::io::{self, ErrorKind, Write};

/// Writes a line to stdout, tolerating a closed downstream pipe.
///
/// `println!` panics when stdout is gone, which is exactly what happens for the
/// very ordinary `ytq list | head`. Rust ignores `SIGPIPE` at startup, so the
/// write returns `BrokenPipe` instead of killing the process, and the default
/// `println!` machinery turns that into a panic message on stderr. Exiting
/// quietly matches what every other command-line tool does in that situation.
pub fn write_line(args: Arguments<'_>) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    let result = handle
        .write_fmt(args)
        .and_then(|()| handle.write_all(b"\n"));

    if let Err(error) = result {
        if error.kind() == ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: failed writing to stdout: {error}");
        std::process::exit(1);
    }
}

/// Drop-in replacement for `println!` that does not panic on a broken pipe.
#[macro_export]
macro_rules! outln {
    () => {
        $crate::output::write_line(format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::output::write_line(format_args!($($arg)*))
    };
}

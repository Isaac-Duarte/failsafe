use std::fmt;

pub fn eprint_send(args: fmt::Arguments<'_>) {
    if std::env::var("FAILSAFE_DEBUG_SEND").is_ok() {
        eprintln!("[failsafe-send]{args}");
    }
}

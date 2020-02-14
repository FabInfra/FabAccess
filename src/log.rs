use slog::{Drain, Logger};
use slog_async;
use slog_term::{TermDecorator, FullFormat};

pub fn init() -> Logger {
    let decorator = TermDecorator::new().build();
    let drain = FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    return slog::Logger::root(drain, o!());
}

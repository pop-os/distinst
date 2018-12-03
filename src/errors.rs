use std::io;

pub trait IoContext<T> {
    fn with_context<F: FnMut(io::Error) -> String>(self, func: F) -> io::Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn with_context<F: FnMut(io::Error) -> String>(self, mut func: F) -> io::Result<T> {
        self.map_err(|why| io::Error::new(why.kind(), func(why)))
    }
}

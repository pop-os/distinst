// NOTE: Possibly make this a crate.

#[derive(SmartDefault)]
pub struct Retry {
    #[default = 3]
    attempts: u64,
    #[default = 1000]
    interval: u64,
}

impl Retry {
    pub fn attempts(mut self, attempts: u64) -> Self {
        self.attempts = attempts;
        self
    }

    pub fn interval(mut self, interval: u64) -> Self {
        self.interval = interval;
        self
    }

    pub fn retry_until_ok<F, T, E>(&self, mut func: F) -> Result<T, E>
    where
        F: FnMut() -> Result<T, E>,
    {
        let duration = ::std::time::Duration::from_millis(self.interval);
        let mut attempt = 0;
        loop {
            match func() {
                Ok(value) => return Ok(value),
                Err(why) => {
                    if attempt == self.attempts {
                        return Err(why);
                    } else {
                        attempt += 1;
                        ::std::thread::sleep(duration);
                    }
                }
            }
        }
    }
}

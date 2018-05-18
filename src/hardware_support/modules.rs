use std::io::{self, Read};
use std::fs::File;

pub struct Module {
    pub name: String,
}

impl Module {
    fn parse(line: &str) -> io::Result<Module> {
        let mut parts = line.split(" ");

        let name = parts.next().ok_or(io::Error::new(
            io::ErrorKind::InvalidData,
            "module name not found"
        ))?;

        Ok(Module {
            name: name.to_string(),
        })
    }

    pub fn all() -> io::Result<Vec<Module>> {
        let mut modules = Vec::new();

        let mut data = String::new();
        File::open("/proc/modules").and_then(|mut file| file.read_to_string(&mut data))?;

        for line in data.lines() {
            let module = Module::parse(line)?;
            modules.push(module);
        }

        Ok(modules)
    }
}

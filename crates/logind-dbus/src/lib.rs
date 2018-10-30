//! Provides a DBus API for interacting with logind, which is useful for doing things such as inhibiting suspension.

#[macro_use]
extern crate cascade;
extern crate dbus;

use dbus::{arg, BusType, Connection, ConnPath};
use std::ops::Deref;

pub struct LoginManager {
    conn: Connection
}

impl Deref for LoginManager {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl LoginManager {
    pub fn new() -> Result<LoginManager, dbus::Error> {
        Ok(Self { conn: Connection::get_private(BusType::System)? })
    }

    pub fn connect(&self) -> LoginManagerConnection {
        LoginManagerConnection {
            conn: self.with_path("org.freedesktop.login1", "/org/freedesktop/login1", 1000)
        }
    }
}

pub struct LoginManagerConnection<'a> {
    conn: ConnPath<'a, &'a Connection>
}

impl<'a> LoginManagerConnection<'a> {
    pub fn inhibit_suspend(&self) -> Result<dbus::OwnedFd, dbus::Error> {
        let mut m = self.conn.method_call_with_args(
            &"org.freedesktop.login1.Manager".into(),
            &"Inhibit".into(),
            |msg| {
                cascade! {
                    arg::IterAppend::new(msg);
                    ..append("idle:shutdown:sleep");
                    ..append("Distinst Installer");
                    ..append("Installing Linux distribution");
                    ..append("block");
                }
            })?;

        m.as_result()?;
        Ok(m.iter_init().read::<dbus::OwnedFd>()?)
    }
}

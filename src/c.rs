extern crate libc;

/// An installer object
#[repr(C)]
pub struct Installer(super::Installer);

/// Create an installer object
#[no_mangle]
pub extern fn installer_new() -> *mut Installer {
    Box::into_raw(Box::new(Installer(super::Installer::new())))
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern fn installer_emit_status(installer: *mut Installer, status: u32) {
    (*installer).0.emit_status(status);
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern fn installer_on_status(installer: *mut Installer, callback: extern "C" fn(status: u32)) {
    (*installer).0.on_status(move |status| {
        callback(status)
    });
}

/// Destroy an installer object
#[no_mangle]
pub unsafe extern fn installer_destroy(installer: *mut Installer) {
    drop(Box::from_raw(installer))
}

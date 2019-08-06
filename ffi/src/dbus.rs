use distinst::dbus_interfaces::LoginManager;
use libc;

#[no_mangle]
pub extern "C" fn distinst_session_inhibit_suspend() -> libc::c_int {
    let manager = match LoginManager::new() {
        Ok(manager) => manager,
        Err(why) => {
            error!("failed to get logind manager: {}", why);
            return -1;
        }
    };

    match manager
        .connect()
        .inhibit_suspend("Distinst Installer", "prevent suspension while installing a distribution")
    {
        Ok(pipe_fd) => pipe_fd.into_fd(),
        Err(why) => {
            error!("failed to suspend: {}", why);
            return -1;
        }
    }
}

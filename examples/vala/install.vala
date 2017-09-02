public static string step_name (Distinst.Step step) {
    switch(step) {
    case Distinst.Step.PARTITION:
        return "Partition";
    case Distinst.Step.FORMAT:
        return "Format";
    case Distinst.Step.EXTRACT:
        return "Extract";
    case Distinst.Step.CONFIGURE:
        return "Configure";
    case Distinst.Step.BOOTLOADER:
        return "Bootloader";
    default:
        return "Unknown";
    }
}

public static int main (string[] args) {
    var installer = new Distinst.Installer ();
    var user_data = 0x12C0FFEE;

    installer.on_error((error) => {
        warning ("Error: %s %s %X", step_name (error.step), strerror (error.err), user_data);
    });

    installer.on_status((status) => {
        warning ("Status: %s %d %X", step_name (status.step), status.percent, user_data);
    });

    var config = Distinst.Config ();
    config.squashfs = "../../bash/filesystem.squashfs";
    config.drive = "/dev/sda";

    installer.install (config);

    return 0;
}

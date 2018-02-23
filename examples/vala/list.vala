public static string level_name (Distinst.LogLevel level) {
    switch(level) {
    case Distinst.LogLevel.TRACE:
        return "Trace";
    case Distinst.LogLevel.DEBUG:
        return "Debug";
    case Distinst.LogLevel.INFO:
        return "Info";
    case Distinst.LogLevel.WARN:
        return "Warn";
    case Distinst.LogLevel.ERROR:
        return "Error";
    default:
        return "Unknown";
    }
}

public static int main (string[] args) {
    Distinst.log((level, message) => {
        stderr.printf ("Log: %s %s\r\n", level_name (level), message);
    });

    Distinst.Disks disks = Distinst.Disks.probe ();
    foreach (unowned Distinst.Disk disk in disks.list()) {
        uint8[] disk_path = disk.get_device_path();
        stdout.printf(
            "%.*s: %d * %d\n",
            disk_path.length,
            (string) disk_path,
            (int)disk.get_sectors(),
            (int)disk.get_sector_size()
        );

        foreach (unowned Distinst.Partition partition in disk.list_partitions()) {
            uint8[] part_path = partition.get_device_path();
            stdout.printf(
                "  %.*s: %d : %d\n",
                part_path.length,
                (string) part_path,
                (int)partition.get_start_sector(),
                (int)partition.get_end_sector()
            );
        }
    }

    return 0;
}

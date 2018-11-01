#[macro_export]
macro_rules! append_packages {
    ($os_release:expr, $install_pkgs:ident { $($detect:tt),+ }) => (
        $(
            if let Some(package) = $detect($os_release) {
                $install_pkgs.push(package);
            }
        )+
    );
}

#[macro_export]
macro_rules! vendor {
    ($os_release:expr, $input:expr => { $($method:tt $pattern:expr => $func:tt),+ }) => (
        $(
            if $input.$method($pattern) {
                return $func($os_release);
            }
        )+
    )
}

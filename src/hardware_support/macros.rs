#[macro_export]
macro_rules! package {
    ($name:tt { $($distro:expr => $package:expr),+ })  => (
        fn $name(distro: &str) -> Option<&'static str> {
            match distro {
                $($distro => Some($package)),+,
                _ => None
            }
        }
    )
}


#[macro_export]
macro_rules! append_packages {
    ($install_pkgs:ident, $distro:expr => { $($detect:tt),+ }) => (
        $(
            if let Some(package) = $detect($distro) {
                $install_pkgs.push(package);
            }
        )+
    );
}

#[macro_export]
macro_rules! vendor {
    ($input:expr, $distro:expr, { $($method:tt $pattern:expr => $func:tt),+ }) => (
        $(
            if $input.$method($pattern) {
                return $func($distro);
            }
        )+
    )
}

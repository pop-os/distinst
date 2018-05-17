#[macro_export]
macro_rules! package {
    (name $distro:expr => $package:expr) => {{
        if OS_RELEASE.name.as_str() == $distro {
            return Some($package);
        }
    }};

    (like $distro:expr => $package:expr) => {{
        if OS_RELEASE.id_like.as_str() == $distro {
            return Some($package);
        }
    }};

    (vendor $vendor:expr => $package:expr) => {{
        if vendor().map_or(false, |vendor| vendor.starts_with($vendor))  {
            return Some($package);
        }
    }};

    ($name:tt { $( $( $field:tt $distro:expr ),+ => $package:expr ),+ })  => (
        fn $name() -> Option<&'static str> {
            $(
                $(
                    package!($field $distro => $package);
                )+
            )+

            None
        }
    );
}


#[macro_export]
macro_rules! append_packages {
    ($install_pkgs:ident { $($detect:tt),+ }) => (
        $(
            if let Some(package) = $detect() {
                $install_pkgs.push(package);
            }
        )+
    );
}

#[macro_export]
macro_rules! vendor {
    ($input:expr => { $($method:tt $pattern:expr => $func:tt),+ }) => (
        $(
            if $input.$method($pattern) {
                return $func();
            }
        )+
    )
}

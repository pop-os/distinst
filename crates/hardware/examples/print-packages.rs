use os_release::OS_RELEASE;

fn main() {
    let mut packages = Vec::new();
    distinst_hardware_support::append_packages(&mut packages, OS_RELEASE.as_ref().unwrap());
    println!("{:?}", packages);
}

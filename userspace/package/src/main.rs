#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_package::{Database, Dependency, Error, Manifest, Name, Version};
use vibrix_syscall as syscall;

fn package_runtime_self_test() -> bool {
    let Ok(core_name) = Name::new(b"core") else {
        return false;
    };
    let Ok(app_name) = Name::new(b"app") else {
        return false;
    };
    let Ok(core) = Manifest::new(core_name, Version::new(1, 4, 0), 4096, [0x11; 32], &[]) else {
        return false;
    };
    let dependency = Dependency {
        name: core_name,
        minimum: Version::new(1, 3, 0),
    };
    let Ok(app) = Manifest::new(
        app_name,
        Version::new(2, 0, 1),
        8192,
        [0x22; 32],
        &[dependency],
    ) else {
        return false;
    };

    let wire = app.encode();
    if Manifest::decode(&wire) != Ok(app) {
        return false;
    }
    let mut corrupt = wire;
    corrupt[500] = 1;
    if Manifest::decode(&corrupt) != Err(Error::Reserved) {
        return false;
    }

    let mut database = Database::new();
    if database.install(app) != Err(Error::MissingDependency) || !database.is_empty() {
        return false;
    }
    if database.install(core).is_err() || database.install(app).is_err() || database.len() != 2 {
        return false;
    }
    let Ok(dependencies) = database.direct_dependencies(app_name) else {
        return false;
    };
    if dependencies != [dependency] {
        return false;
    }
    let mut dependents = [Name::EMPTY; vibrix_package::DATABASE_CAPACITY];
    let Ok(dependent_count) = database.direct_dependents(core_name, &mut dependents) else {
        return false;
    };
    if dependent_count != 1 || dependents[0] != app_name {
        return false;
    }
    if database.remove(core_name) != Err(Error::RequiredByInstalled) {
        return false;
    }
    database.remove(app_name) == Ok(app)
        && database.remove(core_name) == Ok(core)
        && database.is_empty()
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let status = if syscall::getpid() == Ok(1) && package_runtime_self_test() {
        0
    } else {
        1
    };
    let _ = syscall::exit(status);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = syscall::exit(127);
    loop {
        core::hint::spin_loop();
    }
}

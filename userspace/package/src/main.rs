#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_package::{
    Capabilities, CapabilityDeclaration, Database, Dependency, Error, Manifest, Name, Version,
};
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
    let declaration = CapabilityDeclaration::new(
        app_name,
        Capabilities::FILESYSTEM_READ.union(Capabilities::NETWORK),
    );
    if CapabilityDeclaration::decode(&declaration.encode()) != Ok(declaration)
        || !declaration.capabilities.contains(Capabilities::NETWORK)
        || declaration
            .capabilities
            .contains(Capabilities::FILESYSTEM_WRITE)
    {
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
    if database.install(core).is_err() {
        return false;
    }
    if database.install_reviewed(app, declaration, Capabilities::FILESYSTEM_READ)
        != Err(Error::PermissionReviewRequired)
        || database.len() != 1
    {
        return false;
    }
    if database
        .install_reviewed(
            app,
            declaration,
            Capabilities::FILESYSTEM_READ.union(Capabilities::NETWORK),
        )
        .is_err()
        || database.len() != 2
    {
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
    if database.remove(app_name) != Ok(app)
        || database.remove(core_name) != Ok(core)
        || !database.is_empty()
    {
        return false;
    }

    // Exercise the same package objects through the transactional profile
    // API inside the real Ring-3 package probe. Failed dependent-first ordering
    // must preserve the complete installed state.
    if database.install_profile(&[core, app]).is_err() || database.len() != 2 {
        return false;
    }
    if database.remove_profile(&[core_name, app_name]) != Err(Error::RequiredByInstalled)
        || database.len() != 2
        || database.get(core_name).is_none()
        || database.get(app_name).is_none()
    {
        return false;
    }
    database.remove_profile(&[app_name, core_name]).is_ok() && database.is_empty()
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

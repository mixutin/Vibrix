#![no_std]

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vibrix_stack_protector_probe(input: u8) -> u8 {
    let mut local = [0u8; 64];
    for (index, byte) in local.iter_mut().enumerate() {
        *byte = input.wrapping_add(index as u8);
    }
    // Keep the stack object observable to codegen without importing a runtime.
    unsafe { core::ptr::read_volatile(local.as_ptr().add(31)) }
}

use std::any::{type_name, TypeId};
use std::mem::forget;
use std::ptr;

pub fn self_transmute<SRC: 'static, TGT: 'static>(source: SRC) -> TGT {
    if TypeId::of::<SRC>() != TypeId::of::<TGT>() {
        panic!("{} is not {} !", type_name::<SRC>(), type_name::<TGT>());
    }
    let target = unsafe { ptr::read(&source as *const SRC as *const TGT) };
    forget(source);
    target
}

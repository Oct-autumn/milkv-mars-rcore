use core::cell::{RefCell, RefMut};

pub struct UPSafeCell<T> {
    /// 内部数据
    inner: RefCell<T>,
}

unsafe impl<T> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    /// 创建一个新的 UPSafeCell
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: RefCell::new(value),
        }
    }
    /// 获取对内部数据的独占可变引用
    /// （若数据已经被借用，则会 panic）
    pub fn exclusive_access(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }
}

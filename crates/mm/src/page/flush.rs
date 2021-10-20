use core::{marker::PhantomData, mem};

use crate::Page;

use super::PageParam;

pub struct FlushGuard<Param: PageParam> {
    asid: Option<usize>,
    page: Page,
    _maker: PhantomData<Param>,
}

impl<Param: PageParam> FlushGuard<Param> {
    pub fn new(asid: Option<usize>, page: Page) -> Self {
        Self {
            asid,
            page,
            _maker: PhantomData,
        }
    }

    pub fn flush(&self) {
        unsafe {
            Param::flush_tlb(self.asid, Some(self.page.start()));
        }
    }

    pub fn ignore(self) {
        mem::forget(self)
    }
}

impl<Param: PageParam> Drop for FlushGuard<Param> {
    fn drop(&mut self) {
        self.flush();
    }
}

pub struct FlushAllGuard<Param: PageParam> {
    asid: Option<usize>,
    _maker: PhantomData<Param>,
}

impl<Param: PageParam> FlushAllGuard<Param> {
    pub fn new(asid: Option<usize>) -> Self {
        Self {
            asid,
            _maker: PhantomData,
        }
    }

    pub fn flush(&self) {
        unsafe {
            Param::flush_tlb(self.asid, None);
        }
    }
}

impl<Param: PageParam> Drop for FlushAllGuard<Param> {
    fn drop(&mut self) {
        self.flush();
    }
}

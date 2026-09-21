use anyhow::Context;
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId, type_name};

/// 一个标记trait，表示可以存储于窗口状态中的类型
pub trait Global: 'static {
    // This trait is intentionally left empty, by virtue of being a marker trait.
    //
    // Use additional traits with blanket implementations to attach functionality
    // to types that implement `Global`.
}

pub trait ReadGlobal {
    fn take<G: Global>(&mut self) -> Option<Box<G>>;

    /// 返回实现类型的全局实例。
    ///
    /// 如果该类型的全局没有分配，就会陷入恐慌。
    fn require_ref<G: Global>(&self) -> &G;

    fn has_global<G: Global>(&self) -> bool;
}

pub trait ReadGlobalMut {
    /// 返回实现类型的全局实例。
    ///
    /// 如果该类型的全局没有分配，就会陷入恐慌。
    fn require_ref_mut<G: Global>(&mut self) -> &mut G;
}

pub trait WriteGlobal {
    fn set_global<G: Global>(&mut self, value: G);
}

pub trait ReadOrInsertGlobal {
    fn get_global_or_insert_with<F, G: Global>(&mut self, func: F) -> &G
    where
        F: FnOnce() -> G;
}

impl ReadGlobal for FxHashMap<TypeId, Box<dyn Any>> {
    fn take<G: Global>(&mut self) -> Option<Box<G>> {
        let value = self.remove(&TypeId::of::<G>());
        match value {
            Some(value) => value.downcast::<G>().ok(),
            None => None,
        }
    }

    fn require_ref<G: Global>(&self) -> &G {
        self.get(&TypeId::of::<G>())
            .map(|any_state| any_state.downcast_ref::<G>().unwrap())
            .with_context(|| format!("no state of type {} exists", type_name::<G>()))
            .unwrap()
    }

    fn has_global<G: Global>(&self) -> bool {
        self.contains_key(&TypeId::of::<G>())
    }
}

impl ReadGlobalMut for FxHashMap<TypeId, Box<dyn Any>> {
    fn require_ref_mut<G: Global>(&mut self) -> &mut G {
        self.get_mut(&TypeId::of::<G>())
            .map(|any_state| any_state.downcast_mut::<G>().unwrap())
            .with_context(|| format!("no state of type {} exists", type_name::<G>()))
            .unwrap()
    }
}

impl WriteGlobal for FxHashMap<TypeId, Box<dyn Any>> {
    fn set_global<G: Global>(&mut self, value: G) {
        self.insert(TypeId::of::<G>(), Box::new(value));
    }
}

impl ReadOrInsertGlobal for FxHashMap<TypeId, Box<dyn Any>> {
    fn get_global_or_insert_with<F, G: Global>(&mut self, func: F) -> &G
    where
        F: FnOnce() -> G,
    {
        if self.has_global::<G>() {
            self.require_ref()
        } else {
            let global = func();
            self.set_global(global);
            self.require_ref()
        }
    }
}

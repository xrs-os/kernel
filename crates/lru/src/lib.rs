#![feature(test)]
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::hash::{BuildHasher, Hash};
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use hashbrown::{hash_map::DefaultHashBuilder, HashMap};

struct KeyRef<T>(NonNull<T>);

impl<T> KeyRef<T> {
    fn new(key: &T) -> Self {
        unsafe { Self(NonNull::new_unchecked(key as *const T as *mut T)) }
    }
}

impl<T> AsRef<T> for KeyRef<T> {
    fn as_ref(&self) -> &T {
        unsafe { self.0.as_ref() }
    }
}

type Item<K, V> = (MaybeUninit<KeyRef<K>>, V);
pub struct LruCache<K, V, S = DefaultHashBuilder> {
    list: LinkedList<Item<K, V>>,
    map: HashMap<K, NonNull<Node<Item<K, V>>>, S>,
    capacity: usize,
}

// The compiler does not automatically derive Send and Sync for LruCache because it contains
// NonNull. The NonNull are safely encapsulated by LruCache though so we can
// implement Send and Sync for it below.
unsafe impl<K: Send, V: Send, S: Send> Send for LruCache<K, V, S> {}
unsafe impl<K: Sync, V: Sync, S: Sync> Sync for LruCache<K, V, S> {}

impl<K, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            list: LinkedList::new(),
            // The capacity limit is checked after map.insert(),
            // so the capacity of the map will reach capacity + 1
            map: HashMap::with_capacity(capacity + 1),
            capacity,
        }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> LruCache<K, V, S> {
    pub fn with_hasher(capacity: usize, hash_builder: S) -> Self {
        Self {
            list: LinkedList::new(),
            map: HashMap::with_capacity_and_hasher(capacity, hash_builder),
            capacity,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        match self.map.get(key) {
            Some(node) => {
                self.list.move_to_head(*node);
                Some(&(unsafe { node.as_ref() }.element.1))
            }
            None => None,
        }
    }

    pub fn put(&mut self, key: K, value: V) {
        match self.map.get_mut(&key) {
            Some(node) => {
                // key already exists in the map, change the value of node.element.1 (value)
                // and move it to the head of the chain
                unsafe { node.as_mut() }.element.1 = value;
                self.list.move_to_head(*node);
            }
            None => {
                if self.map.len() == self.capacity {
                    //  lru capacity is full, eliminate the most recent unused data
                    if let Some((k, _)) = self.list.pop_back() {
                        self.map.remove(unsafe { k.assume_init() }.as_ref());
                    }
                }
                let mut node = self.list.push_front((MaybeUninit::uninit(), value));
                let occupied_entry = self.map.entry(key).insert(node);
                unsafe { node.as_mut() }.element.0 =
                    MaybeUninit::new(KeyRef::new(occupied_entry.key()));
            }
        }
    }
}

struct Node<T> {
    prev: Option<NonNull<Node<T>>>,
    next: Option<NonNull<Node<T>>>,
    element: T,
}

struct LinkedList<T> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    _maker: PhantomData<Box<Node<T>>>, // drop check
}

impl<T> LinkedList<T> {
    const fn new() -> Self {
        Self {
            head: None,
            tail: None,
            _maker: PhantomData,
        }
    }

    fn move_to_head(&mut self, mut node: NonNull<Node<T>>) {
        unsafe {
            let node_mut = node.as_mut();
            match (node_mut.prev, node_mut.next) {
                (None, None) => {
                    // There is only `node` in the current chain
                    return;
                }
                (None, Some(_)) => {
                    // `node` node is already at the head
                    return;
                }
                (Some(mut prev), None) => {
                    // `node` node at the end
                    prev.as_mut().next = None;
                    self.tail = Some(prev);
                }
                (Some(mut prev), Some(mut next)) => {
                    prev.as_mut().next = Some(next);
                    next.as_mut().prev = Some(prev);
                }
            }

            node_mut.next = self.head;
            node_mut.prev = None;
            self.head.unwrap().as_mut().prev = Some(node);
            self.head = Some(node);
        }
    }

    fn push_front(&mut self, ele: T) -> NonNull<Node<T>> {
        let node = Box::leak(Box::new(Node {
            prev: None,
            next: self.head,
            element: ele,
        }))
        .into();

        match self.head {
            Some(mut old_head) => unsafe { old_head.as_mut() }.prev = Some(node),
            None => self.tail = Some(node),
        }
        self.head = Some(node);
        node
    }

    fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|old_tail| unsafe {
            match old_tail.as_ref().prev {
                Some(mut new_tail) => {
                    // The next pointer at the new end of the chain table is set to None !!!
                    new_tail.as_mut().next = None;
                    self.tail = Some(new_tail);
                }
                None => {
                    // There is only one node in the chain table
                    self.head = None;
                    self.tail = None;
                }
            }
            Box::from_raw(old_tail.as_ptr()).element
        })
    }
}

#[cfg(test)]
mod test {
    use super::LruCache;

    #[test]
    fn test1() {
        let mut lru_cache = LruCache::new(2);
        lru_cache.put(10, 10);
        lru_cache.put(11, 11);
        lru_cache.put(11, 12);
        lru_cache.put(13, 12);
        assert_eq!(lru_cache.get(&11), Some(&12));
        assert_eq!(lru_cache.get(&13), Some(&12));
        assert_eq!(lru_cache.get(&10), None);
    }

    #[test]
    fn test2() {
        let mut lru_cache = LruCache::new(2);
        lru_cache.put(1, 1);
        lru_cache.put(2, 2);
        assert_eq!(lru_cache.get(&1), Some(&1));
        lru_cache.put(3, 3);
        assert_eq!(lru_cache.get(&2), None);
        lru_cache.put(4, 4);
        assert_eq!(lru_cache.get(&1), None);
        assert_eq!(lru_cache.get(&3), Some(&3));
        assert_eq!(lru_cache.get(&4), Some(&4));
    }

    #[test]
    fn test3() {
        let mut lru_cache = LruCache::new(2);
        assert_eq!(lru_cache.get(&100), None);
        lru_cache.put(10, 10);
        lru_cache.put(11, 11);
        lru_cache.put(11, 12);
        assert_eq!(lru_cache.get(&10), Some(&10));
    }

    #[test]
    fn test4() {
        let mut lru_cache = LruCache::new(10);
        lru_cache.put(10, 13);
        lru_cache.put(3, 17);
        lru_cache.put(6, 11);
        lru_cache.put(10, 5);
        lru_cache.put(9, 10);
        assert_eq!(lru_cache.get(&13), None);
        lru_cache.put(2, 19);
        assert_eq!(lru_cache.get(&2), Some(&19));
        assert_eq!(lru_cache.get(&3), Some(&17));
        lru_cache.put(5, 25);
        assert_eq!(lru_cache.get(&8), None);
        lru_cache.put(9, 22);
        lru_cache.put(5, 5);
        lru_cache.put(1, 30);
        assert_eq!(lru_cache.get(&11), None);
        lru_cache.put(9, 12);
        assert_eq!(lru_cache.get(&7), None);
        assert_eq!(lru_cache.get(&5), Some(&5));
        assert_eq!(lru_cache.get(&8), None);
        assert_eq!(lru_cache.get(&9), Some(&12));
        lru_cache.put(4, 30);
        lru_cache.put(9, 3);
        assert_eq!(lru_cache.get(&9), Some(&3));
        assert_eq!(lru_cache.get(&10), Some(&5));
        assert_eq!(lru_cache.get(&10), Some(&5));
        lru_cache.put(6, 14);
        lru_cache.put(3, 1);
        assert_eq!(lru_cache.get(&3), Some(&1));
        lru_cache.put(10, 11);
        assert_eq!(lru_cache.get(&8), None);
        lru_cache.put(2, 14);
        assert_eq!(lru_cache.get(&1), Some(&30));
        assert_eq!(lru_cache.get(&5), Some(&5));
        assert_eq!(lru_cache.get(&4), Some(&30));
        lru_cache.put(11, 4);
        lru_cache.put(12, 24);
        lru_cache.put(5, 18);
        assert_eq!(lru_cache.get(&13), None);
        lru_cache.put(7, 23);
        assert_eq!(lru_cache.get(&8), None);
        assert_eq!(lru_cache.get(&12), Some(&24));
        lru_cache.put(3, 27);
        lru_cache.put(2, 12);
        assert_eq!(lru_cache.get(&5), Some(&18));
        lru_cache.put(2, 9);
        lru_cache.put(13, 4);
        lru_cache.put(8, 18);
        lru_cache.put(1, 7);
        assert_eq!(lru_cache.get(&6), None);
        lru_cache.put(9, 29);
        lru_cache.put(8, 21);
        assert_eq!(lru_cache.get(&5), Some(&18));
        lru_cache.put(6, 30);
        lru_cache.put(1, 12);
        assert_eq!(lru_cache.get(&10), None);
        lru_cache.put(4, 15);
        lru_cache.put(7, 22);
        lru_cache.put(11, 26);
        lru_cache.put(8, 17);
        lru_cache.put(9, 29);
        assert_eq!(lru_cache.get(&5), Some(&18));
        lru_cache.put(3, 4);
        lru_cache.put(11, 30);
        assert_eq!(lru_cache.get(&12), None);
        lru_cache.put(4, 29);
        assert_eq!(lru_cache.get(&3), Some(&4));
        assert_eq!(lru_cache.get(&9), Some(&29));
        assert_eq!(lru_cache.get(&6), Some(&30));
        lru_cache.put(3, 4);
        assert_eq!(lru_cache.get(&1), Some(&12));
        assert_eq!(lru_cache.get(&10), None);
        lru_cache.put(3, 29);
        lru_cache.put(10, 28);
        lru_cache.put(1, 20);
        lru_cache.put(11, 13);
        assert_eq!(lru_cache.get(&3), Some(&29));
        lru_cache.put(3, 12);
        lru_cache.put(3, 8);
        lru_cache.put(10, 9);
        lru_cache.put(3, 26);
        assert_eq!(lru_cache.get(&8), Some(&17));
        assert_eq!(lru_cache.get(&7), Some(&22));
        assert_eq!(lru_cache.get(&5), Some(&18));
        lru_cache.put(13, 17);
        lru_cache.put(2, 27);
        lru_cache.put(11, 15);
        assert_eq!(lru_cache.get(&12), None);
        lru_cache.put(9, 19);
        lru_cache.put(2, 15);
        lru_cache.put(3, 16);
        assert_eq!(lru_cache.get(&1), Some(&20));
        lru_cache.put(12, 17);
        lru_cache.put(9, 1);
        lru_cache.put(6, 19);
        assert_eq!(lru_cache.get(&4), None);
        assert_eq!(lru_cache.get(&5), Some(&18));
        assert_eq!(lru_cache.get(&5), Some(&18));
        lru_cache.put(8, 1);
        lru_cache.put(11, 7);
        lru_cache.put(5, 2);
        lru_cache.put(9, 28);
        assert_eq!(lru_cache.get(&1), Some(&20));
        lru_cache.put(2, 2);
        lru_cache.put(7, 4);
        lru_cache.put(4, 22);
        lru_cache.put(7, 24);
        lru_cache.put(9, 26);
        lru_cache.put(13, 28);
        lru_cache.put(11, 26);
    }

    mod bench {
        extern crate test;
        use test::Bencher;

        #[bench]
        fn bench(b: &mut Bencher) {
            b.iter(|| super::test4());
        }
    }
}

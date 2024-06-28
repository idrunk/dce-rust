use std::collections::BTreeMap;
use std::fmt::{Debug, Display, Error, Formatter};
use std::hash::Hash;
use std::ops::Deref;
use std::sync::{Arc, RwLock, Weak};
use crate::mixed::{DceErr, DceResult};

pub enum TreeTraverBreak {
    Stop,
    Break,
    Skip,
    Continue,
}

pub trait KeyFactory<K> {
    fn key(&self) -> K;
    fn child_of(&self, parent: &Self) -> bool;
}

#[derive(Debug)]
pub struct ATree<E, K> {
    element: RwLock<E>,
    children: RwLock<BTreeMap<K, Arc<ATree<E, K>>>>,
    parent: Weak<ATree<E, K>>,
    own: RwLock<Weak<ATree<E, K>>>,
}

impl<E, K> ATree<E, K>
where E: PartialEq + Debug,
    K: Hash + Clone + Ord + Display + Debug
{
    pub fn set(&self, key: K, element: E) -> DceResult<Arc<ATree<E, K>>> {
        let child = Self::new_with_parent(element, Arc::downgrade(&self.own.read()
            .map_err(DceErr::closed0)?.upgrade().ok_or_else(|| DceErr::closed0("Failed to update Weak to Arc"))?))?;
        self.children.write().map_err(DceErr::closed0)?.insert(key, child.clone());
        Ok(child)
    }

    pub fn set_if_absent(&self, key: K, element: E) -> DceResult<Arc<ATree<E, K>>> {
        if let Some(exists) = self.get(&key) {
            return Ok(exists);
        }
        self.set(key, element)
    }

    pub fn set_by_path(&self, path: Vec<K>, element: E) -> DceResult<Arc<ATree<E, K>>> {
        self.actual_set_by_path(path, element, true)
    }

    pub fn set_by_path_if_absent(&self, path: Vec<K>, element: E) -> DceResult<Arc<ATree<E, K>>> {
        self.actual_set_by_path(path, element, false)
    }

    fn actual_set_by_path(&self, mut path: Vec<K>, element: E, force: bool) -> DceResult<Arc<ATree<E, K>>> {
        let key = path.pop().ok_or_else(|| DceErr::closed0("Cannot get by an empty path"))?;
        let parent = self.get_by_path(&path).ok_or_else(|| DceErr::closed0("Parent not found"))?;
        Ok(if force { parent.set(key, element)? } else { parent.set_if_absent(key, element)? })
    }

    pub fn get(&self, key: &K) -> Option<Arc<ATree<E, K>>> {
        return self.children.read().ok()?.get(key).map(Clone::clone);
    }

    pub fn get_by_path(&self, path: &[K]) -> Option<Arc<ATree<E, K>>> {
        let mut child = self.own.read().ok()?.upgrade()?;
        for key in path { child = child.get(key)?; }
        Some(child)
    }

    pub fn parent(&self) -> Option<Arc<ATree<E, K>>> {
        self.parent.upgrade()
    }

    pub fn parents(&self) -> DceResult<Vec<Arc<ATree<E, K>>>> {
        self.parents_until(None, true)
    }

    pub fn parents_until(&self, until: Option<Arc<ATree<E, K>>>, elder_first: bool) -> DceResult<Vec<Arc<ATree<E, K>>>> {
        let parent = self.own.read().map_err(DceErr::closed0)?.upgrade();
        if parent.is_none() {
            return Ok(vec![]);
        }
        let mut parent = parent.ok_or_else(|| DceErr::closed0("Failed to update Weak to Arc"))?;
        let mut parents = vec![parent.clone()];
        while Some(parent.clone()) != until && match parent.parent() {
            Some(p) => {
                parents.push(p.clone());
                parent = p;
                true
            },
            _ => false
        } {}
        if elder_first { parents.reverse(); }
        Ok(parents)
    }

    pub fn children(&self) -> &RwLock<BTreeMap<K, Arc<ATree<E, K>>>> {
        &self.children
    }

    pub fn contains_key(&self, key: K) -> DceResult<bool> {
        self.children.read().map_err(DceErr::closed0).map(|r| r.contains_key(&key))
    }

    pub fn is_empty(&self) -> DceResult<bool> {
        self.children.read().map_err(DceErr::closed0).map(|r| r.is_empty())
    }

    pub fn remove(&mut self, key: &K) -> Option<Arc<ATree<E, K>>> {
        self.children.write().ok()?.remove(key)
    }

    pub fn traversal(
        &self,
        callback: fn(Arc<ATree<E, K>>) -> DceResult<TreeTraverBreak>,
    ) -> DceResult<()> {
        let mut nodes = vec![self.own.read().map_err(DceErr::closed0)?.upgrade().ok_or_else(|| DceErr::closed0("Failed to update Weak to Arc"))?];
        'outer: while let Some(parent) = nodes.pop() {
            let nodes_len = nodes.len();
            for child in parent.children.read().map_err(DceErr::closed0)?.values() {
                match callback(child.clone())? {
                    TreeTraverBreak::Stop => break 'outer,
                    TreeTraverBreak::Break => break,
                    TreeTraverBreak::Skip => continue,
                    _ => nodes.insert(nodes_len.clone(), child.clone()),
                }
            }
        }
        Ok(())
    }

    pub fn new(element: E) -> DceResult<Arc<ATree<E, K>>> {
        Self::new_with_parent(element, Weak::new())
    }

    fn new_with_parent(element: E, parent: Weak<ATree<E, K>>) -> DceResult<Arc<ATree<E, K>>> {
        let rc = Arc::new(ATree {
            element: RwLock::new(element),
            children: RwLock::new(BTreeMap::new()),
            parent,
            own: RwLock::new(Weak::new()),
        });
        *rc.own.write().map_err(DceErr::closed0)? = Arc::downgrade(&rc);
        Ok(rc)
    }

    /// Build a full tree with given elements
    ///
    /// `elements`
    pub fn build(
        &self,
        mut elements: Vec<E>,
        remains_handler: Option<fn(&ATree<E, K>, Vec<E>)>,
    ) -> DceResult<()>
        where E: KeyFactory<K>,
    {
        let mut parents = vec![self.own.write().map_err(DceErr::closed0)?.upgrade().ok_or_else(|| DceErr::closed0("Failed to update Weak to Arc"))?];
        while let Some(pa) = parents.pop() {
            for i in (0 .. elements.len()).filter(|i| pa.element.read().map_or(false, |e| elements[*i].child_of(&e))).rev().collect::<Vec<_>>() {
                let elem = elements.remove(i);
                parents.push(pa.set(elem.key(), elem)?);
            }
        }
        if let Some(remains_handler) = remains_handler {
            remains_handler(self, elements);
        }
        Ok(())
    }
}

impl<E, K> Deref for ATree<E, K> {
    type Target = RwLock<E>;

    fn deref(&self) -> &Self::Target {
        &self.element
    }
}

impl<E: PartialEq, K> PartialEq for ATree<E, K> {
    fn eq(&self, other: &Self) -> bool {
        self.element.read().map_or(false, |s| other.element.read().map_or(false, |o| s.eq(o.deref())))
    }
}

impl<E: Display, K> Display for ATree<E, K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.element.read().map_err(|_| Error)?)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    impl KeyFactory<u8> for (u8, u8, String) {
        fn key(&self) -> u8 {
            self.0
        }

        fn child_of(&self, parent: &Self) -> bool {
            self.1 == parent.0
        }
    }

    #[test]
    fn build() {
        let root = ATree::new((0, 0, "x".to_string())).unwrap();
        root.set(1, (1, 0, "a".to_string())).unwrap();
        root.set(2, (2, 0, "b".to_string())).unwrap();
        root.set_by_path(vec![8], (8, 0, "h".to_string())).unwrap();
        root.set_by_path(vec![1, 3], (3, 1, "c".to_string())).unwrap();
        root.set_by_path(vec![1, 4], (4, 1, "d".to_string())).unwrap();
        root.set_by_path(vec![8, 5], (5, 8, "e".to_string())).unwrap();
        root.set_by_path(vec![8, 5, 6], (6, 5, "f".to_string())).unwrap();
        root.set_by_path(vec![8, 5, 6], (7, 5, "g".to_string())).unwrap();
        root.set_by_path(vec![1, 3, 9], (9, 3, "i".to_string())).unwrap();
        root.set_by_path(vec![1, 3, 10], (10, 3, "j".to_string())).unwrap();
        println!("{:?}", root);
        root.traversal(|t| {
            eprintln!("t = {:?}", **t); 
            Ok(TreeTraverBreak::Continue)
        }).unwrap();
    }


    impl KeyFactory<&'static str> for &'static str {
        fn key(&self) -> &'static str {
            if let Some(index) = self.rfind('/') {
                return &self[index+1 ..]
            }
            self
        }

        fn child_of(&self, parent: &Self) -> bool {
            if let Some(index) = self.rfind('/') {
                &&self[..index] == parent
            } else {
                parent.is_empty()
            }
        }
    }

    #[test]
    fn get_child() {
        let tree = ATree::new("").unwrap();
        tree.build(vec![
            "hello",
            "hello/world",
            "hello/world/!",
            "hello/rust!",
            "hello/dce/for/rust!",
        ], Some(|tree: &ATree<&'static str, &'static str>, mut remains: Vec<&'static str>| {
            let mut fills: BTreeMap<Vec<&'static str>, &'static str> = BTreeMap::new();
            while let Some(element) = remains.pop() {
                let paths: Vec<_> = element.split("/").collect();
                let last_index = paths.len();
                for i in 0..last_index {
                    let path = paths[..=i].to_vec();
                    if matches!(tree.get_by_path(&path), None) && ! fills.contains_key(&path) {
                        let element = Box::leak(path.clone().join("/").into_boxed_str());
                        fills.insert(path, element);
                    }
                }
            }
            while let Some((paths, nb)) = fills.pop_first() {
                tree.set_by_path(paths, nb).unwrap();
            }
        }),).unwrap();
        let t = tree.get_by_path(&["hello", "world"]).unwrap();
        let t2 = t.get(&"!").unwrap();
        let parents = t2.parents_until(tree.get(&"hello"), true);
        println!("{:#?}", t);
        println!("{:#?}", tree);
        println!("{:#?}", t2);
        println!("{:#?}", parents);
        tree.traversal(|t| {
            eprintln!("t = {:?}", t);
            Ok(TreeTraverBreak::Continue)
        }).unwrap();
    }

}

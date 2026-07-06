use crate::object::{Object, ObjectKind};

pub const NULL_ID: ObjectId = ObjectId(u32::MAX);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjectId(pub u32);

pub struct Arena {
    objects: Vec<Object>,
    free: Vec<u32>,
}

impl Arena {
    pub fn new() -> Self {
        Self { objects: Vec::new(), free: Vec::new() }
    }

    pub fn alloc(&mut self, obj: Object) -> ObjectId {
        if let Some(idx) = self.free.pop() {
            self.objects[idx as usize] = obj;
            ObjectId(idx)
        } else {
            let idx = self.objects.len() as u32;
            assert!(idx < u32::MAX, "arena overflow");
            self.objects.push(obj);
            ObjectId(idx)
        }
    }

    pub fn get(&self, id: ObjectId) -> &Object {
        &self.objects[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: ObjectId) -> &mut Object {
        &mut self.objects[id.0 as usize]
    }

    pub fn live_count(&self) -> usize {
        self.objects.len() - self.free.len()
    }

    // Called by the GC after the mark phase: rebuilds the free list and drops
    // all resources (slot Strings, Rcs) held by unreached objects.
    pub fn sweep(&mut self) {
        self.free.clear();
        for idx in 0..self.objects.len() {
            let obj = &mut self.objects[idx];
            if obj.mark {
                obj.mark = false;
            } else {
                obj.slots.clear();
                obj.kind = ObjectKind::Plain;
                self.free.push(idx as u32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ObjectKind;

    #[test]
    fn alloc_and_get() {
        let mut arena = Arena::new();
        let id = arena.alloc(Object::new(ObjectKind::Integer(42)));
        assert!(matches!(arena.get(id).kind, ObjectKind::Integer(42)));
        assert_eq!(arena.live_count(), 1);
    }

    #[test]
    fn alloc_multiple_distinct_ids() {
        let mut arena = Arena::new();
        let id1 = arena.alloc(Object::new(ObjectKind::Integer(1)));
        let id2 = arena.alloc(Object::new(ObjectKind::Integer(2)));
        assert_ne!(id1, id2);
        assert_eq!(arena.live_count(), 2);
    }

    #[test]
    fn get_mut_allows_slot_modification() {
        let mut arena = Arena::new();
        let id = arena.alloc(Object::new(ObjectKind::Plain));
        arena.get_mut(id).mark = true;
        assert!(arena.get(id).mark);
    }

    #[test]
    fn sweep_reclaims_unmarked_and_reuses_slot() {
        let mut arena = Arena::new();
        let id1 = arena.alloc(Object::new(ObjectKind::Integer(1)));
        let id2 = arena.alloc(Object::new(ObjectKind::Integer(2)));
        // mark only id1
        arena.get_mut(id1).mark = true;
        arena.sweep();
        assert_eq!(arena.live_count(), 1);
        assert!(!arena.get(id1).mark); // mark cleared by sweep

        // allocating again should reuse id2's slot
        let id3 = arena.alloc(Object::new(ObjectKind::Integer(3)));
        assert_eq!(id3, id2);
        assert_eq!(arena.live_count(), 2);
    }

    #[test]
    fn sweep_all_unmarked_empties_arena() {
        let mut arena = Arena::new();
        arena.alloc(Object::new(ObjectKind::Plain));
        arena.alloc(Object::new(ObjectKind::Plain));
        arena.sweep();
        assert_eq!(arena.live_count(), 0);
    }
}

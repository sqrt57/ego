use crate::arena::{Arena, ObjectId, NULL_ID};
use crate::env::Env;
use crate::object::{Object, ObjectKind, Slot, SlotKind};

pub const GC_THRESHOLD: usize = 4096;

pub struct RootSet {
    pub lobby: ObjectId,
    pub nil_id: ObjectId,
    pub true_id: ObjectId,
    pub false_id: ObjectId,
    pub integer_proto: ObjectId,
    pub float_proto: ObjectId,
    pub string_proto: ObjectId,
    pub block_proto: ObjectId,
    pub array_proto: ObjectId,
    pub error_proto: ObjectId,
    pub message_not_understood_proto: ObjectId,
    pub bad_block_activation_proto: ObjectId,
    pub zero_divide_proto: ObjectId,
    pub primitive_error_proto: ObjectId,
    pub stack_roots: Vec<ObjectId>,
    pub activation_envs: Vec<Env>,
}

impl RootSet {
    pub fn new() -> Self {
        Self {
            lobby: NULL_ID,
            nil_id: NULL_ID,
            true_id: NULL_ID,
            false_id: NULL_ID,
            integer_proto: NULL_ID,
            float_proto: NULL_ID,
            string_proto: NULL_ID,
            block_proto: NULL_ID,
            array_proto: NULL_ID,
            error_proto: NULL_ID,
            message_not_understood_proto: NULL_ID,
            bad_block_activation_proto: NULL_ID,
            zero_divide_proto: NULL_ID,
            primitive_error_proto: NULL_ID,
            stack_roots: Vec::new(),
            activation_envs: Vec::new(),
        }
    }
}

pub fn collect(arena: &mut Arena, roots: &RootSet) {
    let mut worklist: Vec<ObjectId> = Vec::new();

    push_if_valid(&mut worklist, roots.lobby);
    push_if_valid(&mut worklist, roots.nil_id);
    push_if_valid(&mut worklist, roots.true_id);
    push_if_valid(&mut worklist, roots.false_id);
    for &id in &roots.stack_roots {
        push_if_valid(&mut worklist, id);
    }
    for env in &roots.activation_envs {
        for &val in env.borrow().values() {
            push_if_valid(&mut worklist, val);
        }
    }

    while let Some(id) = worklist.pop() {
        if arena.get(id).mark {
            continue;
        }
        arena.get_mut(id).mark = true;

        let mut to_visit: Vec<ObjectId> = Vec::new();
        {
            let obj = arena.get(id);
            for slot in &obj.slots {
                if slot.value != NULL_ID {
                    to_visit.push(slot.value);
                }
            }
            if let ObjectKind::Block(block) = &obj.kind {
                if block.captured_self != NULL_ID {
                    to_visit.push(block.captured_self);
                }
                if let Some(resend) = block.captured_resend {
                    if resend != NULL_ID {
                        to_visit.push(resend);
                    }
                }
                let env = block.captures.borrow();
                for &val in env.values() {
                    if val != NULL_ID {
                        to_visit.push(val);
                    }
                }
            }
            if let ObjectKind::Array(elems) = &obj.kind {
                for &e in elems {
                    if e != NULL_ID {
                        to_visit.push(e);
                    }
                }
            }
        }

        for child_id in to_visit {
            if !arena.get(child_id).mark {
                worklist.push(child_id);
            }
        }
    }

    arena.sweep();
}

pub fn alloc_with_gc(arena: &mut Arena, roots: &RootSet, obj: Object) -> ObjectId {
    if arena.live_count() > GC_THRESHOLD {
        collect(arena, roots);
    }
    arena.alloc(obj)
}

/// Allocates a string object with `string_proto` wired as its parent, the
/// same steps every string-producing site (literals, concatenation,
/// exception messageText) needs to repeat.
pub fn make_string(s: impl Into<Box<str>>, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::StringVal(s.into())));
    arena.get_mut(id).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: roots.string_proto,
    });
    id
}

fn push_if_valid(worklist: &mut Vec<ObjectId>, id: ObjectId) {
    if id != NULL_ID {
        worklist.push(id);
    }
}
